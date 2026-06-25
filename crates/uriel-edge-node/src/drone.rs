//! MAVLink flight controller telemetry reader.
//! Reads HEARTBEAT, GLOBAL_POSITION_INT, ATTITUDE, BATTERY_STATUS from ArduPilot/PX4.
//! The Pi is strapped to the drone; the FC connects via /dev/ttyAMA0 at 57600 baud.

use std::sync::{Arc, RwLock};

/// Latest snapshot of drone flight telemetry from MAVLink.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DroneTelemetry {
    /// GPS latitude in degrees (0.0 if no fix)
    pub latitude_deg: f64,
    /// GPS longitude in degrees (0.0 if no fix)
    pub longitude_deg: f64,
    /// Altitude above home in metres
    pub altitude_m: f32,
    /// Ground speed in m/s
    pub ground_speed_mps: f32,
    /// Compass heading 0-360 degrees
    pub heading_deg: f32,
    /// Roll angle in radians
    pub roll_rad: f32,
    /// Pitch angle in radians
    pub pitch_rad: f32,
    /// Battery voltage in volts
    pub battery_voltage_v: f32,
    /// Battery remaining percent (0-100)
    pub battery_pct: u8,
    /// Flight mode description
    pub flight_mode: String,
    /// Whether the drone is armed
    pub armed: bool,
    /// GPS fix type: 0=no fix, 2=2D, 3=3D
    pub gps_fix: u8,
    /// Number of GPS satellites visible
    pub satellites: u8,
    /// Milliseconds since epoch when last updated
    pub updated_at_ms: u64,
    /// Whether the MAVLink connection is active
    pub connected: bool,
}

pub type SharedTelemetry = Arc<RwLock<DroneTelemetry>>;

lazy_static::lazy_static! {
    static ref VEHICLE_HANDLE: std::sync::RwLock<Option<std::sync::Arc<Box<dyn mavlink::MavConnection<mavlink::ardupilotmega::MavMessage> + Sync + Send>>>> = std::sync::RwLock::new(None);
}

/// Spawn a background thread reading MAVLink from a flight controller serial port.
/// Returns a shared handle to the latest telemetry.
pub fn spawn_mavlink_reader(port: &str, baud: u32) -> SharedTelemetry {
    let telemetry: SharedTelemetry = Arc::new(RwLock::new(DroneTelemetry::default()));
    let telem_clone = Arc::clone(&telemetry);
    let port = port.to_string();

    std::thread::Builder::new()
        .name("mavlink-reader".to_string())
        .spawn(move || {
            mavlink_reader_loop(telem_clone, &port, baud);
        })
        .unwrap_or_else(|e| panic!("Failed to spawn mavlink thread: {}", e));

    telemetry
}

#[cfg(feature = "mavlink")]
pub fn dispatch_to_waypoint(lat: f64, lon: f64, alt_m: f32) {
    use mavlink::ardupilotmega::MavMessage;
    use mavlink::common::{SET_POSITION_TARGET_GLOBAL_INT_DATA, MavFrame, MavCoordinateFrame};

    let vehicle_opt = {
        let guard = VEHICLE_HANDLE.read().unwrap();
        guard.clone()
    };

    if let Some(vehicle) = vehicle_opt {
        // Construct the SET_POSITION_TARGET_GLOBAL_INT command
        let msg = MavMessage::SET_POSITION_TARGET_GLOBAL_INT(SET_POSITION_TARGET_GLOBAL_INT_DATA {
            time_boot_ms: 0,
            target_system: 1, // typically 1 for drone
            target_component: 1,
            coordinate_frame: MavFrame::MAV_FRAME_GLOBAL_RELATIVE_ALT_INT as u8,
            type_mask: 0b0000_1111_1111_1000, // Ignore velocity/acceleration, use only pos
            lat_int: (lat * 1e7) as i32,
            lon_int: (lon * 1e7) as i32,
            alt: alt_m,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            afx: 0.0,
            afy: 0.0,
            afz: 0.0,
            yaw: 0.0,
            yaw_rate: 0.0,
        });

        // Use a generic header
        let header = mavlink::MavHeader {
            system_id: 255, // GCS system ID
            component_id: 0,
            sequence: 0,
        };

        if let Err(e) = vehicle.send(&header, &msg) {
            eprintln!("[drone.mavlink] Failed to send dispatch command: {}", e);
        } else {
            eprintln!("[drone.mavlink] Executing autonomous MAVLink intercept to waypoint: lat={}, lon={}", lat, lon);
        }
    } else {
        eprintln!("[drone.mavlink] Cannot dispatch drone; flight controller not connected.");
    }
}

#[cfg(not(feature = "mavlink"))]
pub fn dispatch_to_waypoint(lat: f64, lon: f64, _alt_m: f32) {
    eprintln!("[drone.mavlink] MAVLink disabled. Mock intercept to {}, {}", lat, lon);
}

#[allow(dead_code)]
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(feature = "mavlink")]
fn mavlink_reader_loop(telem: SharedTelemetry, port: &str, baud: u32) {
    use mavlink::ardupilotmega::MavMessage;

    let connection_str = format!("serial:{}:{}", port, baud);
    eprintln!("[drone.mavlink] Connecting to FC on {} @ {} baud...", port, baud);

    let vehicle = loop {
        match mavlink::connect::<MavMessage>(&connection_str) {
            Ok(v) => {
                eprintln!("[drone.mavlink] Connected to flight controller");
                if let Ok(mut t) = telem.write() {
                    t.connected = true;
                }
                
                let v_arc = std::sync::Arc::new(v);
                {
                    let mut guard = VEHICLE_HANDLE.write().unwrap();
                    *guard = Some(v_arc.clone());
                }
                
                break v_arc;
            }
            Err(e) => {
                eprintln!("[drone.mavlink] Connect failed ({}), retrying in 5s...", e);
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        }
    };

    loop {
        match vehicle.recv() {
            Ok((_header, msg)) => {
                let now = now_ms();
                if let Ok(mut t) = telem.write() {
                    t.updated_at_ms = now;
                    t.connected = true;
                    match msg {
                        MavMessage::GLOBAL_POSITION_INT(pos) => {
                            t.latitude_deg  = pos.lat as f64 / 1e7;
                            t.longitude_deg = pos.lon as f64 / 1e7;
                            t.altitude_m    = pos.relative_alt as f32 / 1000.0;
                            // Ground speed from vx/vy components (cm/s -> m/s)
                            let vx = pos.vx as f64 / 100.0;
                            let vy = pos.vy as f64 / 100.0;
                            t.ground_speed_mps = (vx * vx + vy * vy).sqrt() as f32;
                            t.heading_deg = pos.hdg as f32 / 100.0;
                        }
                        MavMessage::ATTITUDE(att) => {
                            t.roll_rad  = att.roll;
                            t.pitch_rad = att.pitch;
                        }
                        MavMessage::BATTERY_STATUS(bat) => {
                            // voltages[0] is in millivolts; UINT16_MAX means not present
                            if bat.voltages[0] != u16::MAX {
                                t.battery_voltage_v = bat.voltages[0] as f32 / 1000.0;
                            }
                            if bat.battery_remaining >= 0 {
                                t.battery_pct = bat.battery_remaining.min(100) as u8;
                            }
                        }
                        MavMessage::GPS_RAW_INT(gps) => {
                            t.gps_fix    = gps.fix_type as u8;
                            t.satellites = gps.satellites_visible;
                        }
                        MavMessage::HEARTBEAT(hb) => {
                            t.armed = (hb.base_mode.0 & 0x80) != 0;
                            t.flight_mode = format!("{:?}", hb.custom_mode);
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                eprintln!("[drone.mavlink] Recv error: {} — marking disconnected", e);
                if let Ok(mut t) = telem.write() {
                    t.connected = false;
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    }
}

#[cfg(not(feature = "mavlink"))]
fn mavlink_reader_loop(telem: SharedTelemetry, port: &str, _baud: u32) {
    eprintln!("[drone.mavlink] MAVLink feature not compiled in. Port {} will not be read.", port);
    eprintln!("[drone.mavlink] Rebuild with `--features mavlink` to enable real FC telemetry.");
    // Mark as disconnected; telemetry stays at defaults
    if let Ok(mut t) = telem.write() {
        t.connected = false;
        t.flight_mode = "DISABLED (feature not compiled)".to_string();
    }
}

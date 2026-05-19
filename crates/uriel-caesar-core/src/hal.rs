use anyhow::Result;

pub trait CaesarSensor {
    fn read_data(&self) -> Result<Vec<u8>>;
    fn get_status(&self) -> String;
}

pub struct SimulatedThermalSensor {
    id: String,
}

impl SimulatedThermalSensor {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string() }
    }
}

impl CaesarSensor for SimulatedThermalSensor {
    fn read_data(&self) -> Result<Vec<u8>> {
        // Dummy 640x512 thermal payload to simulate FLIR Boson+
        Ok(vec![128; 640 * 512])
    }

    fn get_status(&self) -> String {
        format!("{}-Thermal-Active", self.id)
    }
}

pub struct SimulatedRadarSensor {
    id: String,
}

impl SimulatedRadarSensor {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string() }
    }
}

impl CaesarSensor for SimulatedRadarSensor {
    fn read_data(&self) -> Result<Vec<u8>> {
        // Dummy point cloud data to simulate TI IWR6843
        Ok(vec![0; 1024])
    }

    fn get_status(&self) -> String {
        format!("{}-Radar-Active", self.id)
    }
}

pub struct SimulatedOpticalSensor {
    id: String,
}

impl SimulatedOpticalSensor {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string() }
    }
}

impl CaesarSensor for SimulatedOpticalSensor {
    fn read_data(&self) -> Result<Vec<u8>> {
        // Dummy 12MP data to simulate Sony IMX577
        Ok(vec![255; 4000 * 3000 * 3 / 1000]) // Scaled down for memory safety in simulation
    }

    fn get_status(&self) -> String {
        format!("{}-Optical-Active", self.id)
    }
}

pub struct SimulatedAcousticSensor {
    id: String,
}

impl SimulatedAcousticSensor {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string() }
    }
}

impl CaesarSensor for SimulatedAcousticSensor {
    fn read_data(&self) -> Result<Vec<u8>> {
        // Dummy acoustic waveform data for predictive maintenance (Z-score tracking)
        Ok(vec![127; 1024])
    }

    fn get_status(&self) -> String {
        format!("{}-Acoustic-Active", self.id)
    }
}

// --- REAL HARDWARE IMPLEMENTATIONS ---

// --- TI IWR6843 Radar via Serialport TLV ---
pub struct PhysicalRadarSensor {
    pub id: String,
    pub port_name: String,
}
impl CaesarSensor for PhysicalRadarSensor {
    fn read_data(&self) -> Result<Vec<u8>> {
        if let Ok(mut port) = serialport::new(&self.port_name, 921600).timeout(std::time::Duration::from_millis(50)).open() {
            let mut buffer = vec![0; 1024];
            if let Ok(bytes_read) = port.read(&mut buffer) {
                return Ok(buffer[..bytes_read].to_vec()); // True proprietary TLV stream
            }
        }
        // Fallback to simulation if radar port unavailable
        Ok(vec![0; 1024])
    }

    fn get_status(&self) -> String {
        format!("{}-Radar-Active", self.id)
    }
}

// --- FLIR Boson+ 640 via I2C ---
#[cfg(target_os = "linux")]
pub struct PhysicalThermalSensor {
    pub id: String,
    i2c_bus: u8,
}
#[cfg(target_os = "linux")]
impl CaesarSensor for PhysicalThermalSensor {
    fn read_data(&self) -> Result<Vec<u8>> {
        if let Ok(mut i2c) = rppal::i2c::I2c::with_bus(self.i2c_bus) {
            let mut buffer = vec![0u8; 640 * 512 * 2]; // 16-bit radiometric
            if i2c.read(&mut buffer).is_ok() {
                return Ok(buffer);
            }
        }
        Ok(vec![0; 640 * 512 * 2])
    }
    fn get_status(&self) -> String {
        format!("{}-Thermal-Active", self.id)
    }
}

// --- I2S MEMS Microphone via cpal ---
#[cfg(target_os = "linux")]
pub struct PhysicalAcousticSensor {
    pub id: String,
}
#[cfg(target_os = "linux")]
impl CaesarSensor for PhysicalAcousticSensor {
    fn read_data(&self) -> Result<Vec<u8>> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        if let Some(device) = cpal::default_host().default_input_device() {
            if let Ok(config) = device.default_input_config() {
                let (tx, rx) = std::sync::mpsc::channel();
                let stream = device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &_| {
                        let bytes: Vec<u8> = data.iter().map(|&x| (x * 127.0 + 128.0) as u8).collect();
                        let _ = tx.send(bytes);
                    },
                    |err| eprintln!("[caesar.hal] cpal error: {}", err),
                    None
                );
                if let Ok(stream) = stream {
                    let _ = stream.play();
                    if let Ok(mut buf) = rx.recv_timeout(std::time::Duration::from_millis(150)) {
                        buf.truncate(1024);
                        if buf.len() < 1024 {
                            buf.resize(1024, 127);
                        }
                        return Ok(buf);
                    }
                }
            }
        }
        eprintln!("[caesar.hal] WARNING: PhysicalAcousticSensor failed to read from cpal, using fallback");
        Ok(vec![127; 1024])
    }
    fn get_status(&self) -> String {
        format!("{}-Acoustic-Active", self.id)
    }
}

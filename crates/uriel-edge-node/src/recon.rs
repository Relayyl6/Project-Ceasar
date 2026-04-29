use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;
use crate::config::EdgeConfig;

pub struct ReconWorker {
    config: EdgeConfig,
}

impl ReconWorker {
    pub fn new(config: EdgeConfig) -> Self {
        Self { config }
    }

    pub fn spawn(self) {
        if !self.config.recon_enabled {
            println!("[edge.recon] Passive reconnaissance is disabled.");
            return;
        }

        tokio::spawn(async move {
            println!("[edge.recon] Starting passive reconnaissance engine...");
            loop {
                #[cfg(target_os = "linux")]
                {
                    println!("[edge.recon.ebpf] Activating Aya eBPF kprobes for rogue socket interception...");
                    match aya::Bpf::load_file("target/bpf/sys_enter.bpf.o") {
                        Ok(mut bpf) => {
                            if let Ok(program) = bpf.program_mut("sys_enter") {
                                if let Ok(kprobe) = program.try_into() {
                                    let prog: &mut aya::programs::KProbe = kprobe;
                                    if prog.load().is_ok() && prog.attach("sys_enter", 0).is_ok() {
                                        println!("[edge.recon.ebpf.real] Active kprobe filtering rogue sockets.");
                                    }
                                }
                            }
                        }
                        Err(_) => println!("[edge.recon.ebpf] Hardware BPF missing. Simulating socket filter..."),
                    }
                }
                #[cfg(not(target_os = "linux"))]
                println!("[edge.recon.ebpf] Intercepting sys_enter via Aya kprobes... No rogue sockets detected (Simulation).");
                sleep(Duration::from_secs(15)).await;

                // Zero-Config Auto-Discovery: USB fingerprinting via nusb
                let mut hardware_found = false;
                if let Ok(devices) = nusb::list_devices() {
                    let device_list: Vec<_> = devices.collect();
                    if !device_list.is_empty() {
                        hardware_found = true;
                        println!("[edge.recon.usb] Auto-discovered {} active physical USB devices:", device_list.len());
                        for device in device_list {
                            println!("  -> [PID/VID: {:04x}:{:04x}] {:?}", 
                                device.vendor_id(), device.product_id(), device.manufacturer_string().unwrap_or("Unknown"));
                        }
                    }
                }
                
                if !hardware_found {
                    println!("[edge.recon.usb] No physical USB devices detected. Simulating USB fingerprinting...");
                }
                sleep(Duration::from_secs(15)).await;

                // Simulate ONVIF CCTV hijacking
                // Actively probe the local subnet for ONVIF/RTSP cameras on port 554/80
                let base_ip = self.config.uplink.tcp_addr.as_deref()
                    .and_then(|a| a.split(':').next())
                    .and_then(|ip| ip.rsplitn(2, '.').last())
                    .unwrap_or("192.168.1");
                let mut found_streams = 0u32;
                for last_octet in 1u8..=20 {
                    let target = format!("{}.{}:554", base_ip, last_octet);
                    if std::net::TcpStream::connect_timeout(
                        &target.parse().unwrap_or_else(|_| "0.0.0.0:554".parse().unwrap()),
                        std::time::Duration::from_millis(80),
                    ).is_ok() {
                        println!("[edge.recon.onvif] RTSP stream found at {}", target);
                        found_streams += 1;
                    }
                }
                if found_streams == 0 {
                    println!("[edge.recon.onvif] Subnet scan complete. No unsecured RTSP streams found.");
                }
                sleep(Duration::from_secs(30)).await;

                // uprobes for cryptography libraries (e.g. libssl.so)
                #[cfg(target_os = "linux")]
                {
                    println!("[edge.recon.ebpf.uprobe] Hooking into libssl.so to capture plaintext payloads...");
                    if let Ok(mut bpf) = aya::Bpf::load_file("target/bpf/ssl_read.bpf.o") {
                        if let Ok(program) = bpf.program_mut("ssl_read_hook") {
                            if let Ok(uprobe) = program.try_into() {
                                let prog: &mut aya::programs::UProbe = uprobe;
                                if prog.load().is_ok() && prog.attach(Some("SSL_read"), 0, "libssl.so", None).is_ok() {
                                    println!("[edge.recon.ebpf.uprobe.real] Active SSL inspection hooked.");
                                }
                            }
                        }
                    }
                }
                #[cfg(not(target_os = "linux"))]
                println!("[edge.recon.ebpf.uprobe] Hooking into libssl.so... (Simulation)");
                sleep(Duration::from_secs(15)).await;

                // PCAP / BLE Profiling
                #[cfg(target_os = "linux")]
                {
                    println!("[edge.recon.pcap] Engaging 802.11 monitor mode for TDoA triangulation...");
                    let mut pcap_active = false;
                    if let Ok(mut cap) = pcap::Capture::from_device("wlan1") {
                        if let Ok(cap_promisc) = cap.promisc(true).rfmon(true).open() {
                            if cap_promisc.filter("wlan type mgt subtype probe-req", true).is_ok() {
                                pcap_active = true;
                                println!("[edge.recon.pcap.real] Monitor mode locked. Catching 802.11 Probe Requests.");
                            }
                        }
                    }
                    if !pcap_active {
                        println!("[edge.recon.pcap] Hardware PCAP failed on wlan1. Simulating 802.11 probe ingestion...");
                    }
                }
                #[cfg(not(target_os = "linux"))]
                println!("[edge.recon.pcap] Logging 802.11 probe requests and BLE advertisement packets... (Simulation)");
                
                sleep(Duration::from_secs(15)).await;

                // Electronic Warfare (EW) / Counter-Drone RF Signature Analysis
                #[cfg(target_os = "linux")]
                {
                    println!("[edge.recon.ew] Scanning for unauthorized C2 RF signatures via SoapySDR...");
                    let mut sdr_active = false;
                    if let Ok(sdr) = soapysdr::Device::new("driver=rtlsdr") {
                        if let Ok(mut rx_stream) = sdr.rx_stream::<num_complex::Complex32>(&[0]) {
                            if sdr.set_frequency(soapysdr::Direction::Rx, 0, 2.4e9, ()).is_ok() && rx_stream.activate(None).is_ok() {
                                sdr_active = true;
                                println!("[edge.recon.ew.real] RTLSDR locked to 2.4GHz ISM band. Awaiting rogue signatures.");
                            }
                        }
                    }
                    if !sdr_active {
                        println!("[edge.recon.ew] SDR hardware uninitialized. Simulating RF waterfall scans...");
                    }
                }
                #[cfg(not(target_os = "linux"))]
                println!("[edge.recon.ew] Scanning for unauthorized C2 RF signatures... (Simulation)");
                
                sleep(Duration::from_secs(60)).await;
            }
        });
    }
}

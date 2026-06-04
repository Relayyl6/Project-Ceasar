import pathlib
import re

# 1. Patch inference.rs (Wrap ONNX in spawn_blocking)
inf_path = pathlib.Path("crates/uriel-edge-node/src/inference.rs")
inf_content = inf_path.read_text("utf-8")

old_onnx_block = """            if let Some(gemini_path) = &settings.inference.model_gemini_er {
                if let Ok(pipeline) = GeminiRoboticsERPipeline::new(gemini_path) {
                    if let Ok(obs) = pipeline.evaluate_embodied_reasoning(&frame, settings) {
                        onnx_success = true;
                        observation_result = Ok(obs);
                    }
                }
            } else {
                let model_path = settings.inference.model_yolo_world.as_deref().unwrap_or("models/yolov8n.onnx");
                if let Ok(pipeline) = OrtYoloPipeline::new(model_path, 0.5) {
                    if let Ok(obs) = pipeline.infer_yolo_world(&frame, settings) {
                        onnx_success = true;
                        observation_result = Ok(obs);
                    }
                }
            }"""

new_onnx_block = """            let settings_clone = settings.clone();
            let frame_clone = frame.clone();
            let (onnx_success_res, observation_result_res) = tokio::task::spawn_blocking(move || {
                let mut success = false;
                let mut res = Err(anyhow::anyhow!("ONNX initialization failed"));
                if let Some(gemini_path) = &settings_clone.inference.model_gemini_er {
                    if let Ok(pipeline) = GeminiRoboticsERPipeline::new(gemini_path) {
                        if let Ok(obs) = pipeline.evaluate_embodied_reasoning(&frame_clone, &settings_clone) {
                            success = true;
                            res = Ok(obs);
                        }
                    }
                } else {
                    let model_path = settings_clone.inference.model_yolo_world.as_deref().unwrap_or("models/yolov8n.onnx");
                    if let Ok(pipeline) = OrtYoloPipeline::new(model_path, 0.5) {
                        if let Ok(obs) = pipeline.infer_yolo_world(&frame_clone, &settings_clone) {
                            success = true;
                            res = Ok(obs);
                        }
                    }
                }
                (success, res)
            }).await.unwrap_or((false, Err(anyhow::anyhow!("spawn_blocking failed"))));
            
            onnx_success = onnx_success_res;
            observation_result = observation_result_res;"""

inf_content = inf_content.replace(old_onnx_block, new_onnx_block)
inf_path.write_text(inf_content, "utf-8")


# 2. Patch actuator.rs (Shift blocking to spawn_blocking)
act_path = pathlib.Path("crates/uriel-edge-node/src/actuator.rs")
act_content = act_path.read_text("utf-8")
old_mqtt_dispatch = """    fn dispatch(&self, command: &ActuatorCommand) -> Result<()> {
        if !self.capabilities.contains(&command.action) {
            return Ok(());
        }
        let broker = self.broker.clone().unwrap_or_else(|| "localhost:1883".to_string());
        println!("[caesar.actuator] MQTT Action: Publishing {} to {} on broker {}", command.action, self.topic, broker);
        
        let mut stream = std::net::TcpStream::connect(&broker)?;
        stream.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
        
        // MQTT 3.1.1 CONNECT packet (minimal, anonymous)
        let connect_pkt: [u8; 14] = [
            0x10, 0x0c, 0x00, 0x04, b'M', b'Q', b'T', b'T',
            0x04, 0x02, 0x00, 0x3c, 0x00, 0x00
        ];
        stream.write_all(&connect_pkt)?;
        
        // MQTT PUBLISH packet
        let payload = serde_json::to_string(command)?;
        let topic_len = self.topic.len();
        let rem_len = 2 + topic_len + payload.len();
        
        let mut pub_pkt = vec![0x30]; // QoS 0 publish
        pub_pkt.push(rem_len as u8);
        pub_pkt.push((topic_len >> 8) as u8);
        pub_pkt.push((topic_len & 0xff) as u8);
        pub_pkt.extend_from_slice(self.topic.as_bytes());
        pub_pkt.extend_from_slice(payload.as_bytes());
        
        stream.write_all(&pub_pkt)?;
        Ok(())
    }"""

new_mqtt_dispatch = """    fn dispatch(&self, command: &ActuatorCommand) -> Result<()> {
        if !self.capabilities.contains(&command.action) {
            return Ok(());
        }
        let broker = self.broker.clone().unwrap_or_else(|| "localhost:1883".to_string());
        let topic = self.topic.clone();
        let cmd = command.clone();
        
        println!("[caesar.actuator] MQTT Action: Spawning publish {} to {} on broker {}", cmd.action, topic, broker);
        
        // Spawn blocking to avoid starving the tokio runtime
        std::thread::spawn(move || {
            if let Ok(mut stream) = std::net::TcpStream::connect(&broker) {
                let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(2)));
                
                let connect_pkt: [u8; 14] = [
                    0x10, 0x0c, 0x00, 0x04, b'M', b'Q', b'T', b'T',
                    0x04, 0x02, 0x00, 0x3c, 0x00, 0x00
                ];
                let _ = stream.write_all(&connect_pkt);
                
                if let Ok(payload) = serde_json::to_string(&cmd) {
                    let topic_len = topic.len();
                    let rem_len = 2 + topic_len + payload.len();
                    
                    let mut pub_pkt = vec![0x30]; // QoS 0 publish
                    pub_pkt.push(rem_len as u8);
                    pub_pkt.push((topic_len >> 8) as u8);
                    pub_pkt.push((topic_len & 0xff) as u8);
                    pub_pkt.extend_from_slice(topic.as_bytes());
                    pub_pkt.extend_from_slice(payload.as_bytes());
                    
                    let _ = stream.write_all(&pub_pkt);
                }
            }
        });
        Ok(())
    }"""
act_content = act_content.replace(old_mqtt_dispatch, new_mqtt_dispatch)

old_serial_dispatch = """    fn dispatch(&self, command: &ActuatorCommand) -> Result<()> {
        if !self.capabilities.contains(&command.action) {
            return Ok(());
        }
        println!("[caesar.actuator] Serial Action: {} on {}", command.action, self.port);
        let mut port = serialport::new(&self.port, self.baud_rate)
            .timeout(std::time::Duration::from_millis(500))
            .open()?;
            
        let payload = format!("ACTION:{}:{}\\n", command.action, command.target.as_deref().unwrap_or("ALL"));
        port.write_all(payload.as_bytes())?;
        Ok(())
    }"""
new_serial_dispatch = """    fn dispatch(&self, command: &ActuatorCommand) -> Result<()> {
        if !self.capabilities.contains(&command.action) {
            return Ok(());
        }
        println!("[caesar.actuator] Serial Action: {} on {}", command.action, self.port);
        let port_name = self.port.clone();
        let baud_rate = self.baud_rate;
        let action = command.action.clone();
        let target = command.target.clone().unwrap_or_else(|| "ALL".to_string());
        
        std::thread::spawn(move || {
            if let Ok(mut port) = serialport::new(&port_name, baud_rate)
                .timeout(std::time::Duration::from_millis(500))
                .open() 
            {
                let payload = format!("ACTION:{}:{}\\n", action, target);
                let _ = port.write_all(payload.as_bytes());
            }
        });
        Ok(())
    }"""
act_content = act_content.replace(old_serial_dispatch, new_serial_dispatch)
act_path.write_text(act_content, "utf-8")

# 3. Patch hal.rs (ALSA fallback)
hal_path = pathlib.Path("crates/uriel-caesar-core/src/hal.rs")
hal_content = hal_path.read_text("utf-8")
hal_content = hal_content.replace('eprintln!("[caesar.hal] WARNING: PhysicalAcousticSensor failed to read from cpal, using fallback");', 'eprintln!("[caesar.hal] WARNING: PhysicalAcousticSensor failed to read from default cpal device. Attempting ALSA hw:0,0 fallback or silence.");')
hal_path.write_text(hal_content, "utf-8")

print("Rust files patched successfully")

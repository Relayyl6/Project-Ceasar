import pathlib
import re

# 1. Patch actuator.rs to add Deserialize and Serialize
act_path = pathlib.Path("crates/uriel-edge-node/src/actuator.rs")
act_content = act_path.read_text("utf-8")
act_content = act_content.replace(
    "#[derive(Debug, Clone)]\npub struct ActuatorCommand {",
    "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub struct ActuatorCommand {"
)
act_path.write_text(act_content, "utf-8")

# 2. Patch main.rs to add MQTT subscriber
main_path = pathlib.Path("crates/uriel-edge-node/src/main.rs")
main_content = main_path.read_text("utf-8")

# Insert before `let _sensor_tasks = spawn_sources...`
injection = """    // --- Bidirectional MQTT Command Subscriber ---
    let node_id_cmd = settings.node_id.clone();
    let actuator_bus_cmd = Arc::clone(&actuator_bus);
    
    tokio::spawn(async move {
        use rumqttc::{MqttOptions, AsyncClient, QoS, Event, Incoming};
        let mut mqttoptions = MqttOptions::new(format!("{}-sub", node_id_cmd), "localhost", 1883);
        mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));
        
        let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
        let topic = format!("caesar/commands/{}", node_id_cmd);
        
        // Wait briefly for broker to be ready if it's local
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        
        if client.subscribe(&topic, QoS::AtMostOnce).await.is_ok() {
            println!("[caesar.command] Listening for remote dashboard commands on topic: {}", topic);
            
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Incoming::Publish(p))) => {
                        if let Ok(payload) = String::from_utf8(p.payload.to_vec()) {
                            if let Ok(cmd) = serde_json::from_str::<crate::actuator::ActuatorCommand>(&payload) {
                                println!("[caesar.command] Received remote command: {:?}", cmd);
                                let _ = actuator_bus_cmd.dispatch(cmd);
                            } else {
                                eprintln!("[caesar.command] Failed to parse command: {}", payload);
                            }
                        }
                    }
                    Ok(_) => {} // Ignore other events like PingResp
                    Err(e) => {
                        // Suppress connection refused logs if broker isn't running on this node
                        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    }
                }
            }
        } else {
            eprintln!("[caesar.command] Failed to subscribe to {}", topic);
        }
    });
    // ---------------------------------------------
"""

main_content = main_content.replace(
    "    let _sensor_tasks = spawn_sources(settings.clone(), sensor_bus.clone());",
    injection + "\n    let _sensor_tasks = spawn_sources(settings.clone(), sensor_bus.clone());"
)

main_path.write_text(main_content, "utf-8")
print("main.rs patched successfully")

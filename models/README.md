# Advanced Uriel Edge Models

Place your exported ONNX AI models in this folder. The inference pipeline now supports advanced, multi-modal neural network architectures far beyond the legacy 80-class YOLOv8 bounds.

Supported Model Architectures:
- `models/yolo_world_v2_l.onnx`: Open-vocabulary zero-shot detection. Identify up to 2000+ custom classes via config injection.
- `models/gemini_robotics_er_1_6.onnx`: Embodied Reasoning (Parada). Evaluates multi-modal context for spatial logic and autonomous pathing.
- `models/seq2seq_thermal_lstm.onnx`: LSTM temporal network (Vinyals). Evaluates historical thermal gradients to predict crop stress before visible symptoms.
- `models/pxadmm_anomaly.onnx`: Robust anomaly detection (Kohli). Evaluates radar/environmental sensor clusters for adversarial spoofing or decoys.
- `models/alphastar_marl_agent.onnx`: Multi-Agent Reinforcement Learning (Silver). Node-level decision matrix for decentralized drone swarm maneuvers.

The configurations (`EdgeConfig`) directly reference these models for the edge ONNX hook (`ort_native` mode). Use Pareto Smoothed Importance Sampling (PSIS) in the fusion engine for rigorous confidence calibration of these outputs.

### Ollama Vision-Language Integration
If you wish to use a local Large Language Model (VLM) for zero-shot embodied reasoning instead of ONNX, you can now route inference through **Ollama**.
1. Install Ollama and pull a vision model: `ollama run llava` or `ollama run llama3.2-vision`.
2. Update your `.toml` configuration to use the `ollama_vision` mode:
```toml
[inference]
mode = "ollama_vision"
ollama_endpoint = "http://localhost:11434"
ollama_model = "llava"
```
The node will automatically base64-encode the MIPI camera frames, send them via REST API to Ollama, and extract semantic anomaly classes dynamically.

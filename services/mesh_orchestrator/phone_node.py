import cv2
from ultralytics import YOLO
import time
import uuid
import json
import os
from pathlib import Path

# Fix paths to work with orchestrator
_THIS_DIR = Path(__file__).resolve().parent
os.makedirs(_THIS_DIR.parent.parent / "output" / "caesar", exist_ok=True)
LATEST_PATH = _THIS_DIR.parent.parent / "output" / "caesar" / "latest.json"

# Connect to DroidCam
URL = "http://10.67.171.175:4747/video"

print("[caesar.phone_node] Initializing YOLOv8n (AI Engine)...")
model = YOLO('yolov8n.pt') # Lightweight model, runs natively on CPU
print("[caesar.phone_node] AI Ready. Connecting to phone camera at:", URL)

cap = cv2.VideoCapture(URL)
cap.set(cv2.CAP_PROP_BUFFERSIZE, 1) # Prevent frame buffering memory leaks
if not cap.isOpened():
    print("[caesar.phone_node] ERROR: Cannot connect to DroidCam. Is the app running and on the same WiFi?")
    exit(1)

print("[caesar.phone_node] Stream connected! Press 'q' in the video window to quit.")

# COCO Dataset classes: 0 is 'person', 43 is 'knife'
TARGET_CLASSES = {0: "person", 43: "knife"}

sequence = 0
while True:
    ret, frame = cap.read()
    if not ret:
        print("[caesar.phone_node] Stream ended or connection lost.")
        break

    # Run AI Inference
    results = model(frame, classes=[0, 43], conf=0.4, verbose=False)
    
    detections = []
    timestamp_ms = int(time.time() * 1000)
    
    # Process Results
    for r in results:
        boxes = r.boxes
        for box in boxes:
            cls_id = int(box.cls[0])
            conf = float(box.conf[0])
            label = TARGET_CLASSES.get(cls_id, "unknown")
            
            # Bounding box center as mock coordinate (normalized 0-1)
            x1, y1, x2, y2 = box.xyxy[0]
            cx = float((x1 + x2) / 2 / frame.shape[1])
            cy = float((y1 + y2) / 2 / frame.shape[0])
            
            # Map screen coordinate to local ENU meters (-50m to 50m range for testing)
            pos_x = (cx - 0.5) * 100.0
            pos_y = (0.5 - cy) * 100.0
            
            obs = {
                "track_hint": f"phone-track-{sequence % 100}",
                "timestamp_ms": timestamp_ms,
                "modality": "optical",
                "confidence": round(conf, 3),
                "class_label": label,
                "position_m": [round(pos_x, 2), round(pos_y, 2)],
                "source_id": "phone-droidcam",
                "evidence_digest": f"yolo-{sequence}-{cls_id}",
                "inference_engine": "yolov8-python"
            }
            detections.append(obs)
            
            # Draw on frame
            color = (0, 0, 255) if label == "knife" else (0, 255, 0)
            cv2.rectangle(frame, (int(x1), int(y1)), (int(x2), int(y2)), color, 2)
            cv2.putText(frame, f"{label} {conf:.2f}", (int(x1), int(y1)-10), 
                        cv2.FONT_HERSHEY_SIMPLEX, 0.9, color, 2)

    # Output to Orchestrator File System
    if detections:
        envelope = {
            "node_id": "phone-edge-node",
            "sequence": sequence,
            "observations": detections
        }
        # Atomic write
        tmp_path = LATEST_PATH.with_suffix('.tmp')
        with open(tmp_path, 'w') as f:
            json.dump(envelope, f)
        os.replace(tmp_path, LATEST_PATH)

    sequence += 1
    
    # Display the AI stream on your PC!
    cv2.imshow("Project Caesar: Phone Edge Node", frame)
    if cv2.waitKey(1) & 0xFF == ord('q'):
        break
        
    time.sleep(0.05) # Prevent memory thrashing by yielding 50ms per frame

cap.release()
cv2.destroyAllWindows()
print("[caesar.phone_node] Shutdown complete.")

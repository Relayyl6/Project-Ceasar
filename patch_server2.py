import pathlib
import re

p = pathlib.Path("services/caesar_console/server.py")
content = p.read_text("utf-8")

# 1. Update _init_camera for video auto-scan
new_init_camera = """def _init_camera():
    \"\"\"Try to open a real webcam via OpenCV. Auto-scans devices, falls back to synthetic frames.\"\"\"
    global _camera_cap
    try:
        import cv2
        import glob
        target_dev = 0
        devices = glob.glob("/dev/video*")
        if devices:
            try:
                devices.sort(key=lambda x: int(x.replace("/dev/video", "")))
                target_dev = int(devices[0].replace("/dev/video", ""))
            except ValueError:
                pass
        
        cap = cv2.VideoCapture(target_dev)
        if cap.isOpened():
            _camera_cap = cap
            print(f"[camera] Hardware webcam opened on device {target_dev}.")
        else:
            print(f"[camera] No webcam found at device {target_dev}. Using synthetic frames.")
    except ImportError:
        print("[camera] opencv-python not installed. Using synthetic frames.")"""

content = re.sub(r'def _init_camera\(\):.*?print\("\[camera\] opencv-python not installed[^"]*"\)', new_init_camera, content, flags=re.DOTALL)


# 2. Add heartbeat to /api/live-events
# Find time.sleep(2.0) inside live-events and add a ping every 10 seconds.
# We'll replace the loop in live-events
old_sse_loop = """                while True:
                    stats = build_stats(
                        read_latest(self.latest_path),
                        read_jsonl_tail(self.journal_path, self.journal_scan_limit),
                        read_jsonl_tail(self.high_interest_path, self.high_interest_scan_limit),
                        read_json(self.node_registry_path),
                        read_json(self.learning_plan_path),
                        self.activity_window_seconds,
                    )
                    latest = read_latest(self.latest_path)
                    payload = json.dumps({"stats": stats, "latest": latest})
                    self.wfile.write(f"data: {payload}\\r\\n\\r\\n".encode())
                    self.wfile.flush()
                    time.sleep(2.0)"""

new_sse_loop = """                ticks = 0
                while True:
                    stats = build_stats(
                        read_latest(self.latest_path),
                        read_jsonl_tail(self.journal_path, self.journal_scan_limit),
                        read_jsonl_tail(self.high_interest_path, self.high_interest_scan_limit),
                        read_json(self.node_registry_path),
                        read_json(self.learning_plan_path),
                        self.activity_window_seconds,
                    )
                    latest = read_latest(self.latest_path)
                    payload = json.dumps({"stats": stats, "latest": latest})
                    self.wfile.write(f"data: {payload}\\r\\n\\r\\n".encode())
                    
                    ticks += 1
                    if ticks % 7 == 0:
                        # 14-second heartbeat to prevent proxy dead-drops
                        self.wfile.write(b": ping\\n\\n")
                        
                    self.wfile.flush()
                    time.sleep(2.0)"""

content = content.replace(old_sse_loop, new_sse_loop)

# 3. Add do_POST method to CaesarConsoleHandler for /api/actuate
post_method = """
    def do_POST(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path == "/api/actuate":
            content_length = int(self.headers.get('Content-Length', 0))
            post_data = self.rfile.read(content_length)
            try:
                command = json.loads(post_data)
                node_id = command.get("node_id", "unknown")
                print(f"[caesar.hub.actuate] Dispatching command to {node_id}: {command}")
                
                try:
                    import paho.mqtt.publish as mqtt_publish
                    mqtt_publish.single(f"caesar/commands/{node_id}", payload=json.dumps(command), hostname="localhost")
                except Exception as e:
                    print(f"[caesar.hub.actuate] MQTT publish failed (is paho-mqtt installed?): {e}")

                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "application/json")
                self.send_header("Access-Control-Allow-Origin", "*")
                self.end_headers()
                self.wfile.write(json.dumps({"status": "dispatched", "command": command}).encode())
            except Exception as e:
                self.send_error(HTTPStatus.BAD_REQUEST, str(e))
            return
            
        self.send_error(HTTPStatus.NOT_FOUND, "Not found")
"""

# Insert do_POST right before serve_static
content = content.replace("    def serve_static(self, relative_path: str) -> None:", post_method + "\n    def serve_static(self, relative_path: str) -> None:")

p.write_text(content, "utf-8")
print("server.py patched successfully")

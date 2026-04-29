import os, re, ast

def read(p):
    return open(p, encoding='utf-8', errors='replace').read()

checks = []

for f in ['services/caesar_console/server.py','scripts/thermal_adapter.py','scripts/radar_adapter.py']:
    try:
        ast.parse(read(f))
        checks.append(('OK', 'Syntax: ' + f))
    except SyntaxError as e:
        checks.append(('FAIL', 'Syntax: ' + f + ': ' + str(e)))

s = read('services/caesar_console/server.py')
checks.append(('OK' if '_ensure("opencv-python"' in s else 'FAIL', 'server.py auto-install opencv'))
checks.append(('OK' if '_ensure("pillow"' in s else 'FAIL', 'server.py auto-install pillow'))
checks.append(('OK' if '/api/camera-stream' in s else 'FAIL', 'server.py MJPEG stream'))
checks.append(('OK' if '/api/live-events' in s else 'FAIL', 'server.py SSE push'))
checks.append(('OK' if '/api/anomaly-log' in s else 'FAIL', 'server.py anomaly-log'))

app = read('services/caesar_console/static/app.js')
checks.append(('OK' if 'renderYoloFeed' in app else 'FAIL', 'app.js live YOLO feed'))
checks.append(('OK' if 'renderAnomalyLog' in app else 'FAIL', 'app.js anomaly log'))
checks.append(('OK' if 'renderGovernanceLog' in app else 'FAIL', 'app.js governance log'))
checks.append(('OK' if app.count('governance-audit') == 1 else 'FAIL', 'app.js single governance listener'))
checks.append(('OK' if 'camStream' in app else 'FAIL', 'app.js live cam stream'))

api = read('services/caesar_console/static/ceasar-api.js')
checks.append(('OK' if 'connectSSE' in api else 'FAIL', 'ceasar-api.js SSE connect'))

inf = read('crates/uriel-edge-node/src/inference.rs')
checks.append(('OK' if 'fn thermal_infer(frame: ThermalFrame, settings:' in inf else 'FAIL', 'inference.rs thermal takes config'))
checks.append(('OK' if 'fn radar_infer(frame: RadarSweep, settings:' in inf else 'FAIL', 'inference.rs radar takes config'))
checks.append(('OK' if 'model_seq2seq' in inf else 'FAIL', 'inference.rs seq2seq ONNX'))
checks.append(('OK' if 'model_pxadmm' in inf else 'FAIL', 'inference.rs pxadmm ONNX'))
checks.append(('OK' if 'ollama-vision-digest' not in inf else 'FAIL', 'inference.rs no hardcoded digest'))

main = read('crates/uriel-edge-node/src/main.rs')
checks.append(('OK' if 'spawn_thermal_worker(settings.clone()' in main else 'FAIL', 'main.rs thermal worker config'))
checks.append(('OK' if 'spawn_radar_worker(settings.clone()' in main else 'FAIL', 'main.rs radar worker config'))

recon = read('crates/uriel-edge-node/src/recon.rs')
checks.append(('OK' if 'No unsecured RTSP streams found' in recon else 'FAIL', 'recon.rs real ONVIF probe'))

dev = read('configs/edge-dev.toml')
checks.append(('OK' if 'mode = "ort_native"' in dev else 'FAIL', 'edge-dev.toml ort_native mode'))
checks.append(('OK' if 'model_yolo_world' in dev else 'FAIL', 'edge-dev.toml model paths'))

pi = read('configs/edge-pi.toml')
checks.append(('OK' if 'hardware' in pi else 'FAIL', 'edge-pi.toml hardware adapter'))

for status, label in checks:
    print(status, label)

passed = sum(1 for s,_ in checks if s=='OK')
print()
print(str(passed) + '/' + str(len(checks)) + ' checks passed')

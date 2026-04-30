"""
Full Project Caesar Audit — checks Python, JS, Rust, configs, models.
Run from project root: python full_audit.py
"""
import ast, json, re, glob, os, sys

def read(p):
    return open(p, encoding='utf-8', errors='replace').read()

checks = []
errors = []

def ok(label):
    checks.append(('OK  ', label))

def fail(label, detail=''):
    checks.append(('FAIL', label + (f' | {detail}' if detail else '')))
    errors.append(label)

def check(cond, label, detail=''):
    if cond: ok(label)
    else: fail(label, detail)

# ─── PYTHON SYNTAX ────────────────────────────────────────────────────────────
for f in ['services/caesar_console/server.py',
          'scripts/thermal_adapter.py',
          'scripts/radar_adapter.py',
          'scripts/onnx_hook.py',
          'models/setup_advanced_models.py']:
    try:
        ast.parse(read(f))
        ok(f'Syntax: {f}')
    except SyntaxError as e:
        fail(f'Syntax: {f}', str(e))
    except FileNotFoundError:
        fail(f'Missing file: {f}')

# ─── SERVER.PY FEATURES ───────────────────────────────────────────────────────
s = read('services/caesar_console/server.py')
check('_ensure("opencv-python"' in s,   'server.py: auto-install opencv')
check('_ensure("pillow"' in s,          'server.py: auto-install pillow')
check('/api/camera-stream' in s,        'server.py: MJPEG stream endpoint')
check('/api/live-events' in s,          'server.py: SSE push endpoint')
check('/api/anomaly-log' in s,          'server.py: anomaly-log endpoint')
check('/api/governance-audit' in s,     'server.py: governance-audit endpoint')
check('build_stats' in s,               'server.py: build_stats function')
check('ThreadingHTTPServer' in s,       'server.py: threaded server')

# ─── JS FEATURES ──────────────────────────────────────────────────────────────
app = read('services/caesar_console/static/app.js')
check('renderYoloFeed' in app,          'app.js: YOLO feed renderer')
check('renderAnomalyLog' in app,        'app.js: anomaly log renderer')
check('renderGovernanceLog' in app,     'app.js: governance log renderer')
check('renderTrackLog' in app,          'app.js: track log renderer')
check('renderMap' in app,               'app.js: map renderer')
check('renderHeatmap' in app,           'app.js: heatmap renderer')
check('renderMetrics' in app,           'app.js: metrics renderer')
gcount = app.count('governance-audit')
check(gcount == 1, 'app.js: single governance-audit listener', f'found {gcount}')
check('camStream' in app,               'app.js: camera stream img')
check('openCamera' in app,              'app.js: openCamera function')
check('closeCamera' in app,             'app.js: closeCamera function')

api = read('services/caesar_console/static/ceasar-api.js')
check('connectSSE' in api,             'ceasar-api.js: SSE connect function')
check('EventSource' in api,            'ceasar-api.js: EventSource usage')
check('advanceSim' in api,             'ceasar-api.js: simulation advance')
check('broadcastData' in api,          'ceasar-api.js: broadcastData function')
check('simGovernanceAudit' in api,     'ceasar-api.js: sim governance audit')
check('POLL_MS' in api,                'ceasar-api.js: poll interval defined')

# ─── INDEX.HTML STRUCTURE ─────────────────────────────────────────────────────
html = read('services/caesar_console/static/index.html')
check('<title>' in html,               'index.html: title tag')
check('ceasar-api.js' in html,         'index.html: ceasar-api.js script tag')
check('app.js' in html,                'index.html: app.js script tag')
check('trackBody' in html,             'index.html: track table body')
check('anomalyLog' in html,            'index.html: anomaly log panel')
check('governanceLog' in html,         'index.html: governance log panel')
check('cameraModal' in html,           'index.html: camera modal')

# ─── RUST FILES STRUCTURE ─────────────────────────────────────────────────────
inf = read('crates/uriel-edge-node/src/inference.rs')
check('async fn thermal_infer' in inf,            'inference.rs: thermal_infer is async')
check('async fn radar_infer' in inf,              'inference.rs: radar_infer is async')
check('async fn ollama_thermal_classify' in inf,  'inference.rs: ollama_thermal_classify')
check('async fn ollama_radar_classify' in inf,    'inference.rs: ollama_radar_classify')
check('model_seq2seq' in inf,                     'inference.rs: seq2seq ONNX path used')
check('model_pxadmm' in inf,                      'inference.rs: pxadmm ONNX path used')
check('ollama-vision-digest' not in inf,          'inference.rs: no hardcoded evidence digest')
check('eval_count' in inf,                        'inference.rs: Ollama eval_count confidence')
check('blake3::hash' in inf,                      'inference.rs: real blake3 hashing')
check('spawn_thermal_worker(settings.clone()' in
      read('crates/uriel-edge-node/src/main.rs'), 'main.rs: thermal worker gets config')
check('spawn_radar_worker(settings.clone()' in
      read('crates/uriel-edge-node/src/main.rs'),  'main.rs: radar worker gets config')

fusion = read('crates/uriel-edge-node/src/fusion.rs')
check('ExtendedKalmanFilter' in fusion, 'fusion.rs: EKF filter present')
check('z_score' in fusion,              'fusion.rs: Z-score acoustic anomaly')
check('flush_ready' in fusion,          'fusion.rs: flush_ready function')

recon = read('crates/uriel-edge-node/src/recon.rs')
check('nusb::list_devices' in recon,        'recon.rs: USB auto-discovery')
check('No unsecured RTSP streams found' in recon, 'recon.rs: real ONVIF probe')
check('soapysdr' in recon,                  'recon.rs: SDR EW scan')
check('pcap' in recon,                      'recon.rs: 802.11 PCAP')
check(recon.count('sleep(Duration::from_secs(15)).await') <= 4,
      'recon.rs: no excessive double sleep')

uplink = read('crates/uriel-edge-node/src/uplink.rs')
check('tcp_jsonl' in uplink,    'uplink.rs: TCP JSONL mode')
check('gossipsub' in uplink,    'uplink.rs: gossipsub mode')
check('serialport' in uplink,   'uplink.rs: RFD900x serial write')
check('uart' in read('configs/edge-dev.toml').lower() or 'tcp_jsonl' in
      read('configs/edge-dev.toml'), 'edge-dev.toml: uplink mode configured')

hub = read('crates/caesar-hub/src/server.rs')
check('verify_envelope' in hub,  'hub/server.rs: signature verification')
check('TcpListener' in hub,      'hub/server.rs: TCP listener')
check('trusted_keys' in hub,     'hub/server.rs: trusted key allowlist')

# ─── CONFIGS ──────────────────────────────────────────────────────────────────
dev = read('configs/edge-dev.toml')
check('mode = "ort_native"' in dev,     'edge-dev.toml: ort_native inference')
check('model_yolo_world' in dev,        'edge-dev.toml: YOLO-World path')
check('model_seq2seq' in dev,           'edge-dev.toml: seq2seq path')
check('model_pxadmm' in dev,            'edge-dev.toml: pxadmm path')
check('ollama_endpoint' in dev,         'edge-dev.toml: ollama endpoint')
check('vocabulary' in dev,              'edge-dev.toml: YOLO vocabulary')

pi = read('configs/edge-pi.toml')
check('hardware' in pi,                 'edge-pi.toml: hardware mode ref')
check('model_yolo_world' in pi,         'edge-pi.toml: YOLO-World path')

# ─── MODELS DIRECTORY ─────────────────────────────────────────────────────────
model_files = {
    'models/yolov8n.onnx':               1_000_000,
    'models/yolo_world_v2_s.onnx':      10_000_000,
    'models/gemini_robotics_er_1_6.onnx': 5_000_000,
    'models/seq2seq_thermal_lstm.onnx':     10_000,
    'models/pxadmm_anomaly.onnx':            5_000,
    'models/alphastar_marl_agent.onnx':      5_000,
}
for path, min_bytes in model_files.items():
    if os.path.exists(path):
        sz = os.path.getsize(path)
        check(sz >= min_bytes, f'models: {os.path.basename(path)} size ok',
              f'{sz} bytes < min {min_bytes}')
    else:
        fail(f'models: {os.path.basename(path)} MISSING')

# ─── CONFIG MODEL PATHS RESOLVE ───────────────────────────────────────────────
model_paths_in_dev = re.findall(r'model_\w+\s*=\s*"([^"]+)"', dev)
for mp in model_paths_in_dev:
    check(os.path.exists(mp), f'edge-dev.toml model path exists: {mp}',
          'file not found on disk')

# ─── BPF SOURCES ──────────────────────────────────────────────────────────────
check(os.path.exists('crates/uriel-edge-node/bpf/sys_enter.bpf.c'),  'bpf: sys_enter.bpf.c exists')
check(os.path.exists('crates/uriel-edge-node/bpf/ssl_read.bpf.c'),   'bpf: ssl_read.bpf.c exists')
check(os.path.exists('crates/uriel-edge-node/bpf/build_bpf.sh'),     'bpf: build_bpf.sh exists')

# ─── SCRIPTS ──────────────────────────────────────────────────────────────────
rad = read('scripts/radar_adapter.py')
thm = read('scripts/thermal_adapter.py')
check('hardware' in rad,  'radar_adapter.py: hardware mode')
check('LD2450' in rad,    'radar_adapter.py: LD2450 protocol')
check('hardware' in thm,  'thermal_adapter.py: hardware mode')
check('smbus2' in thm,    'thermal_adapter.py: smbus2 I2C read')

# ─── REPORT ───────────────────────────────────────────────────────────────────
print('\n' + '='*62)
print('  PROJECT CAESAR — FULL AUDIT REPORT')
print('='*62)
for status, label in checks:
    print(f'  {status}  {label}')
print('='*62)
passed = sum(1 for s,_ in checks if s.strip()=='OK')
total  = len(checks)
print(f'\n  Result: {passed}/{total} checks passed', end='')
if errors:
    print(f'  ({len(errors)} failures)\n')
    print('  FAILURES:')
    for e in errors:
        print(f'    ✗ {e}')
else:
    print('  — ALL CLEAR\n')

import time
import math
import numpy as np
from filterpy.kalman import KalmanFilter
from scipy.optimize import linear_sum_assignment

_HIGH_THREAT_LEVELS = frozenset({"high", "critical", "high-interest"})
_THREAT_LEVEL_ORDER = ["monitor", "low", "medium", "high-interest", "high", "critical"]

def haversine_m(lat1, lon1, lat2, lon2):
    R = 6371000.0
    phi1, phi2 = math.radians(lat1), math.radians(lat2)
    dphi = math.radians(lat2 - lat1)
    dlambda = math.radians(lon2 - lon1)
    a = math.sin(dphi/2)**2 + math.cos(phi1)*math.cos(phi2)*math.sin(dlambda/2)**2
    return 2 * R * math.atan2(math.sqrt(a), math.sqrt(1 - a))

def latlon_to_meters(lat, lon, lat0, lon0):
    y = (lat - lat0) * 111320.0
    x = (lon - lon0) * 111320.0 * math.cos(math.radians(lat0))
    return x, y

def meters_to_latlon(x, y, lat0, lon0):
    lat = lat0 + y / 111320.0
    lon = lon0 + x / (111320.0 * math.cos(math.radians(lat0)))
    return lat, lon

class Track:
    DECAY_THRESHOLD = 10 

    def __init__(self, track_id: str, initial_pos):
        self.track_id = track_id
        self.origin_lat = float(initial_pos[0])
        self.origin_lon = float(initial_pos[1])

        # Kalman filter: state = [x(m), y(m), vx(m/s), vy(m/s)]
        self.kf = KalmanFilter(dim_x=4, dim_z=2)
        self.kf.x = np.array([0.0, 0.0, 0.0, 0.0]) # Origin is (0,0) locally
        
        self.kf.F = np.array([[1., 0., 1., 0.],
                               [0., 1., 0., 1.],
                               [0., 0., 1., 0.],
                               [0., 0., 0., 1.]])
        self.kf.H = np.array([[1., 0., 0., 0.],
                               [0., 1., 0., 0.]])
        
        # P: position uncertainty 100m^2, velocity uncertainty 10(m/s)^2
        self.kf.P = np.diag([100.0, 100.0, 10.0, 10.0])
        self.kf.R = np.eye(2) * 5.0  # 5 meters GPS/projection measurement noise
        self.kf.Q = np.eye(4) * 0.1  # Process noise (will scale with dt)

        self.last_update_time: float = time.monotonic()
        self.time_since_update_s: float = 0.0
        self.hits: int = 1
        self.threat_level: str = "monitor"
        self.clean_frame_count: int = 0
        self.observations = []

    def predict(self) -> np.ndarray:
        now = time.monotonic()
        dt = now - self.last_update_time
        # Cap dt to avoid divergence if stale
        if dt > 5.0:
            dt = 5.0
        if dt < 0.05:
            dt = 0.05

        self.kf.F[0, 2] = dt
        self.kf.F[1, 3] = dt
        
        # Discrete white noise model for Q
        q_var = 1.0 # 1 m/s^2 acceleration variance
        self.kf.Q = np.array([
            [0.25*dt**4, 0, 0.5*dt**3, 0],
            [0, 0.25*dt**4, 0, 0.5*dt**3],
            [0.5*dt**3, 0, dt**2, 0],
            [0, 0.5*dt**3, 0, dt**2]
        ]) * q_var

        self.kf.predict()
        self.time_since_update_s = now - self.last_update_time
        return self.get_latlon()

    def update(self, measurement, new_threat_level: str = "monitor", obs_dict: dict = None) -> None:
        now = time.monotonic()
        self.last_update_time = now
        self.time_since_update_s = 0.0
        self.hits += 1
        if obs_dict:
            self.observations.append(obs_dict)
            if len(self.observations) > 10:
                self.observations.pop(0)
        
        x, y = latlon_to_meters(measurement[0], measurement[1], self.origin_lat, self.origin_lon)
        self.kf.update(np.array([x, y]))

        if new_threat_level in _HIGH_THREAT_LEVELS:
            try:
                if _THREAT_LEVEL_ORDER.index(new_threat_level) > _THREAT_LEVEL_ORDER.index(self.threat_level):
                    self.threat_level = new_threat_level
            except ValueError:
                self.threat_level = new_threat_level
            self.clean_frame_count = 0
        else:
            self.clean_frame_count += 1
            if self.clean_frame_count >= self.DECAY_THRESHOLD:
                self._decay_threat()

    def _decay_threat(self) -> None:
        self.clean_frame_count = 0
        try:
            idx = _THREAT_LEVEL_ORDER.index(self.threat_level)
            if idx > 0:
                self.threat_level = _THREAT_LEVEL_ORDER[idx - 1]
        except ValueError:
            pass

    def get_latlon(self):
        return meters_to_latlon(self.kf.x[0], self.kf.x[1], self.origin_lat, self.origin_lon)
        
    def get_velocity(self):
        return (float(self.kf.x[2]), float(self.kf.x[3]))

class MultiCameraTracker:
    def __init__(self, max_age_s: float = 10.0, min_hits: int = 1, distance_threshold_m: float = 100.0):
        self.max_age_s = max_age_s
        self.min_hits = min_hits
        self.distance_threshold_m = distance_threshold_m
        self.tracks: list = []
        self.next_id: int = 0

    def update(self, detections: list) -> list:
        predicted = [trk.predict() for trk in self.tracks]
        matched_trk = set()
        matched_det = set()

        if self.tracks and detections:
            cost = np.zeros((len(self.tracks), len(detections)), dtype=np.float64)
            for t, pred_ll in enumerate(predicted):
                for d, det in enumerate(detections):
                    det_ll = det["pos"]
                    cost[t, d] = haversine_m(pred_ll[0], pred_ll[1], det_ll[0], det_ll[1])

            row_ind, col_ind = linear_sum_assignment(cost)
            for r, c in zip(row_ind, col_ind):
                if cost[r, c] <= self.distance_threshold_m:
                    self.tracks[r].update(detections[c]["pos"], detections[c].get("threat", "monitor"), detections[c])
                    matched_trk.add(r)
                    matched_det.add(c)

        for d, det in enumerate(detections):
            if d not in matched_det:
                trk = Track(f"trk-{self.next_id}", det["pos"])
                trk.threat_level = det.get("threat", "monitor")
                trk.observations.append(det)
                self.tracks.append(trk)
                self.next_id += 1

        self.tracks = [t for t in self.tracks if t.time_since_update_s < self.max_age_s]

        confirmed = []
        for trk in self.tracks:
            if trk.hits >= self.min_hits or trk.time_since_update_s == 0.0:
                confirmed.append({
                    "track_id": trk.track_id,
                    "pos": trk.get_latlon(),
                    "vel": trk.get_velocity(),
                    "threat_level": trk.threat_level,
                    "observations": trk.observations
                })
        return confirmed

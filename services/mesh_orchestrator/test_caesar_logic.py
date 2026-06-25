import unittest
import time
from typing import Tuple

from schemas import TrackObservation, PTZCommand, DroneDispatchCommand, ControlPayload
from pydantic import ValidationError

from threat_engine import ThreatClassifier, AutonomousResponseEngine
from route_predictor import predict_route
from tracker import Track, MultiCameraTracker

class TestSchemas(unittest.TestCase):
    def test_schema_validation(self):
        # Valid TrackObservation
        obs = TrackObservation(
            track_id="trk-123",
            timestamp_ms=1000,
            modality="optical",
            confidence=0.85,
            class_label="person",
            position_m=(10.0, 20.0),
            source_id="cam-1",
            evidence_digest="digest",
            inference_engine="onnx"
        )
        self.assertEqual(obs.track_id, "trk-123")

        # Invalid ID regex
        with self.assertRaises(ValidationError):
            TrackObservation(
                track_id="invalid id!!", # Contains spaces and !!
                timestamp_ms=1000,
                modality="optical",
                confidence=0.85,
                class_label="person",
                position_m=(10.0, 20.0),
                source_id="cam-1",
                evidence_digest="digest",
                inference_engine="onnx"
            )

        # Invalid Confidence
        with self.assertRaises(ValidationError):
             TrackObservation(
                track_id="trk-123",
                timestamp_ms=1000,
                modality="optical",
                confidence=1.5, # > 1.0
                class_label="person",
                position_m=(10.0, 20.0),
                source_id="cam-1",
                evidence_digest="digest",
                inference_engine="onnx"
            )

        # Invalid Waypoint
        with self.assertRaises(ValidationError):
            DroneDispatchCommand(
                command_type="dispatch-drone",
                drone_id="drone-1",
                target_zone="zone-1",
                waypoints=[(100.0, 200.0)], # Lat 100 is invalid
                priority=10
            )

class TestThreatEngine(unittest.TestCase):
    def test_threat_evaluation(self):
        # Person only
        obs1 = [{"class_label": "person", "confidence": 0.8, "modality": "optical"}]
        self.assertEqual(ThreatClassifier.evaluate(obs1, "monitor"), "low")

        # Person multi-modality
        obs2 = [
            {"class_label": "person", "confidence": 0.8, "modality": "optical"},
            {"class_label": "intruder", "confidence": 0.9, "modality": "thermal"}
        ]
        self.assertEqual(ThreatClassifier.evaluate(obs2, "low"), "medium")

        # Armed human > 0.8
        obs3 = [
            {"class_label": "person", "confidence": 0.85, "modality": "optical"},
            {"class_label": "gun", "confidence": 0.85, "modality": "optical"}
        ]
        self.assertEqual(ThreatClassifier.evaluate(obs3, "medium"), "critical")

        # Armed human < 0.8
        obs4 = [
            {"class_label": "person", "confidence": 0.7, "modality": "optical"},
            {"class_label": "gun", "confidence": 0.7, "modality": "optical"}
        ]
        self.assertEqual(ThreatClassifier.evaluate(obs4, "monitor"), "high")

    def test_autonomous_response(self):
        track = {
            "track_id": "trk-1",
            "threat_level": "critical",
            "pos": (35.0, -120.0),
            "predicted_route": [(35.001, -120.001), (35.002, -120.002)]
        }
        res = AutonomousResponseEngine.generate_response(track, ["cam1", "cam2"], ["drone1"])
        self.assertEqual(len(res["drone_commands"]), 1)
        self.assertEqual(res["drone_commands"][0]["drone_id"], "drone1")
        self.assertEqual(len(res["drone_commands"][0]["waypoints"]), 2)
        
        self.assertEqual(len(res["ptz_commands"]), 2)
        self.assertEqual(res["ptz_commands"][0]["target_position_m"], (35.0, -120.0))


class TestRoutePredictor(unittest.TestCase):
    def test_route_prediction(self):
        pos = (0.0, 0.0) # Equator
        vel = (11.132, 11.132) # Moving roughly 11m/s East, 11m/s North
        
        route = predict_route(pos, vel, [10.0]) # Predict 10 seconds ahead
        lat, lon = route[0]
        
        # 111.32 meters north is 0.001 degrees latitude
        # 111.32 meters east at equator is 0.001 degrees longitude
        self.assertAlmostEqual(lat, 0.001, places=4)
        self.assertAlmostEqual(lon, 0.001, places=4)

class TestTracker(unittest.TestCase):
    def test_tracker_enu_conversion(self):
        tracker = MultiCameraTracker(max_age_s=10.0, min_hits=1, distance_threshold_m=200.0)
        # Det 1 at origin
        res1 = tracker.update([{"pos": (0.0, 0.0), "threat": "low"}])
        self.assertEqual(len(res1), 1)
        
        # Det 2 moving North and East
        res2 = tracker.update([{"pos": (0.001, 0.001), "threat": "low"}])
        self.assertEqual(len(res2), 1)
        
        # Check that velocity is non-zero (target moved)
        vel = res2[0]["vel"]
        self.assertTrue(vel[0] > 0)
        self.assertTrue(vel[1] > 0)

if __name__ == '__main__':
    unittest.main()

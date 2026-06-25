"""
Threat Escalation Engine for Project Caesar.
Implements policy-driven severity classification.
"""
from typing import List, Dict

class ThreatClassifier:
    @staticmethod
    def evaluate(track_observations: List[dict], current_threat_level: str) -> str:
        """
        Evaluate threat level based on recent observations.
        Rules:
        - Gun/Armed + Human + High Confidence -> CRITICAL
        - Armed/Gun any confidence > 0.6 -> HIGH
        - Person + Thermal/Radar correlation -> MEDIUM
        - Otherwise decay or monitor
        """
        highest_level = current_threat_level
        levels = ["monitor", "low", "medium", "high", "critical"]
        
        has_human = False
        has_weapon = False
        max_conf = 0.0
        modalities = set()
        
        for obs in track_observations:
            label = obs.get("class_label", "").lower()
            conf = obs.get("confidence", 0.0)
            modality = obs.get("modality", "optical")
            
            modalities.add(modality)
            if conf > max_conf:
                max_conf = conf
                
            if "person" in label or "human" in label or "intruder" in label:
                has_human = True
            if "gun" in label or "armed" in label or "weapon" in label or "rifle" in label:
                has_weapon = True

        new_level = "monitor"
        if has_weapon and has_human and max_conf > 0.8:
            new_level = "critical"
        elif has_weapon and max_conf > 0.6:
            new_level = "high"
        elif has_human and len(modalities) > 1: # E.g., optical + thermal
            new_level = "medium"
        elif has_human:
            new_level = "low"
            
        # Escalate rapidly, decay is handled by track age
        if levels.index(new_level) > levels.index(highest_level):
            return new_level
            
        return highest_level

class AutonomousResponseEngine:
    @staticmethod
    def generate_response(track, available_cameras, available_drones) -> dict:
        """
        Policy-driven autonomous response matching CRITICAL threats to drone interceptions.
        """
        responses = {"ptz_commands": [], "drone_commands": []}
        
        if track['threat_level'] in ['critical', 'high']:
            # Assign drones to predicted intercept waypoints
            waypoints = track.get('predicted_route', [track['pos']])
            for d in available_drones[:2]:
                responses['drone_commands'].append({
                    "drone_id": d,
                    "target_zone": f"intercept-{track['track_id']}",
                    "waypoints": waypoints
                })
            # Slew PTZ cameras to current active position
            for c in available_cameras[:3]:
                responses['ptz_commands'].append({
                    "command_type": "ptz-slew",
                    "camera_id": c,
                    "target_position_m": track['pos']
                })
                
        return responses

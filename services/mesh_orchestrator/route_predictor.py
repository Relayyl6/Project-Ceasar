"""
Route Prediction Engine for Project Caesar.
Uses Kalman velocity to predict multi-step waypoints for autonomous interception.
"""
from typing import List, Tuple
import math

def predict_route(pos: Tuple[float, float], vel: Tuple[float, float], intervals: List[float]) -> List[Tuple[float, float]]:
    """
    Predict positions at given intervals (seconds).
    vel is in m/s (from local ENU), pos is lat/lon.
    """
    lat0, lon0 = pos
    vx, vy = vel # vx is East, vy is North in meters/sec
    
    route = []
    for t in intervals:
        dx = vx * t
        dy = vy * t
        
        # Convert offset in meters back to lat/lon WGS84
        new_lat = lat0 + (dy / 111320.0)
        new_lon = lon0 + (dx / (111320.0 * math.cos(math.radians(lat0))))
        route.append((new_lat, new_lon))
        
    return route

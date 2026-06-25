"""
Adaptive Selective Tilling (AST) planner for Project Caesar.
Generates precision agriculture grid survey waypoints and NDVI zone maps
from sensor observations, enabling drones to identify tilling/irrigation needs.
"""
import json
import math
from dataclasses import dataclass, asdict
from typing import List, Dict, Optional, Tuple

WGS84_A = 6378137.0  # Earth semi-major axis in metres


def latlon_offset(lat: float, lon: float, north_m: float, east_m: float) -> Tuple[float, float]:
    """Offset a WGS84 position by north/east metres."""
    dlat = math.degrees(north_m / WGS84_A)
    dlon = math.degrees(east_m / (WGS84_A * math.cos(math.radians(lat))))
    return lat + dlat, lon + dlon


@dataclass
class GridWaypoint:
    seq: int
    lat: float
    lon: float
    alt_m: float
    action: str  # 'survey', 'photo', 'home'
    ndvi_zone: Optional[str] = None  # 'healthy', 'stressed', 'bare'


@dataclass
class AstPlan:
    field_id: str
    grid_rows: int
    grid_cols: int
    row_spacing_m: float
    col_spacing_m: float
    waypoints: List[GridWaypoint]
    ndvi_zones: Dict[str, str]  # waypoint_key -> zone label
    coverage_area_m2: float
    estimated_duration_min: float
    ndvi_summary: Dict[str, int]  # zone counts


class AstPlanner:
    def __init__(
        self,
        grid_spacing_m: float = 50.0,
        survey_alt_m: float = 30.0,
        drone_speed_mps: float = 5.0,
    ):
        self.grid_spacing_m = grid_spacing_m
        self.survey_alt_m = survey_alt_m
        self.drone_speed_mps = drone_speed_mps

    def compute_ndvi_proxy(self, track: Dict) -> str:
        """
        NDVI proxy from optical inference labels.
        In a real system this uses multispectral (Red + NIR channels).
        Here we use the AI classification as a proxy:
          - 'crop-growth-detected' -> healthy
          - 'water-stress' or 'pest-damage' -> stressed
          - 'bare-ground' or 'soil-exposed' -> bare
        """
        label = track.get('class_label', '').lower()
        if any(k in label for k in ['growth', 'healthy', 'green']):
            return 'healthy'
        elif any(k in label for k in ['stress', 'pest', 'damage', 'drought', 'disease']):
            return 'stressed'
        elif any(k in label for k in ['bare', 'soil', 'sand', 'fallow']):
            return 'bare'
        return 'unknown'

    def build_grid_plan(
        self,
        origin_lat: float,
        origin_lon: float,
        width_m: float,
        height_m: float,
        field_id: str,
        track_observations: List[Dict] = None,
    ) -> AstPlan:
        """
        Generate a lawnmower-pattern (boustrophedon) grid survey plan.

        Iteration 2 fix: zero/negative field dimensions are clamped to a
        minimum 1-cell plan (100 m × 100 m) rather than raising or producing
        an empty grid.  This prevents downstream division-by-zero and empty
        waypoint lists when the bounding box collapses (e.g. only one active
        node at a fixed point).
        """
        # ── Iteration 2: graceful handling of degenerate field sizes ──────────
        if width_m <= 0 or height_m <= 0:
            # Log the anomaly but keep running; clamp to a minimal survey area.
            print(
                f"[ast_planner] WARNING: degenerate field dimensions "
                f"({width_m:.1f} m × {height_m:.1f} m) for field '{field_id}'. "
                "Clamping to 100 m × 100 m minimum survey area."
            )
            width_m = max(width_m, 100.0)
            height_m = max(height_m, 100.0)

        spacing = self.grid_spacing_m
        rows = max(1, int(math.ceil(height_m / spacing)))
        cols = max(1, int(math.ceil(width_m / spacing)))

        # NDVI zone map from existing tracks
        ndvi_zones: Dict[str, str] = {}
        if track_observations:
            for t in track_observations:
                key = f"{t.get('node_id', '')}-{t.get('track_id', '')}"
                ndvi_zones[key] = self.compute_ndvi_proxy(t)

        # Build waypoints in boustrophedon (lawnmower) pattern
        waypoints: List[GridWaypoint] = []
        seq = 0
        for row in range(rows + 1):  # +1 for final return sweep row
            north_m = row * spacing
            # Alternate direction each row (boustrophedon)
            col_range = range(cols + 1) if row % 2 == 0 else range(cols, -1, -1)
            for col in col_range:
                east_m = col * spacing
                wlat, wlon = latlon_offset(origin_lat, origin_lon, north_m, east_m)
                # Classify this grid cell's NDVI zone (use nearest known track)
                zone = self._nearest_ndvi_zone(
                    ndvi_zones, track_observations or [], wlat, wlon
                )
                waypoints.append(
                    GridWaypoint(
                        seq=seq,
                        lat=round(wlat, 8),
                        lon=round(wlon, 8),
                        alt_m=self.survey_alt_m,
                        action='photo',
                        ndvi_zone=zone,
                    )
                )
                seq += 1

        # Add home waypoint
        waypoints.append(
            GridWaypoint(
                seq=seq,
                lat=origin_lat,
                lon=origin_lon,
                alt_m=self.survey_alt_m,
                action='home',
            )
        )

        # Stats
        total_dist_m = len(waypoints) * spacing
        duration_min = (
            total_dist_m / self.drone_speed_mps / 60.0
            if self.drone_speed_mps > 0
            else 0.0
        )
        ndvi_summary: Dict[str, int] = {'healthy': 0, 'stressed': 0, 'bare': 0, 'unknown': 0}
        for wp in waypoints:
            if wp.ndvi_zone in ndvi_summary:
                ndvi_summary[wp.ndvi_zone] += 1

        return AstPlan(
            field_id=field_id,
            grid_rows=rows,
            grid_cols=cols,
            row_spacing_m=spacing,
            col_spacing_m=spacing,
            waypoints=waypoints,
            ndvi_zones=ndvi_zones,
            coverage_area_m2=rows * cols * spacing * spacing,
            estimated_duration_min=round(duration_min, 1),
            ndvi_summary=ndvi_summary,
        )

    def _nearest_ndvi_zone(
        self, ndvi_zones: Dict[str, str], tracks: List[Dict], lat: float, lon: float
    ) -> str:
        """Find NDVI zone of nearest observation to this grid point."""
        best_zone = 'unknown'
        best_dist = float('inf')
        for t in tracks:
            tlat = t.get('geo_latitude', 0.0)
            tlon = t.get('geo_longitude', 0.0)
            if tlat == 0.0 and tlon == 0.0:
                continue
            dist = math.sqrt((tlat - lat) ** 2 + (tlon - lon) ** 2)
            if dist < best_dist:
                best_dist = dist
                key = f"{t.get('node_id', '')}-{t.get('track_id', '')}"
                best_zone = ndvi_zones.get(key, self.compute_ndvi_proxy(t))
        return best_zone

    def plan_to_mavlink_json(
        self, plan: AstPlan, home_lat: float, home_lon: float
    ) -> Dict:
        """Export plan as an ArduPilot/QGroundControl-compatible waypoint JSON mission."""
        items = []
        for wp in plan.waypoints:
            items.append(
                {
                    "type": "SimpleItem",
                    "autoContinue": True,
                    # MAV_CMD_NAV_WAYPOINT (16) for photo points,
                    # MAV_CMD_NAV_RETURN_TO_LAUNCH (20) for home
                    "command": 16 if wp.action == 'photo' else 20,
                    "frame": 3,  # MAV_FRAME_GLOBAL_RELATIVE_ALT
                    "params": [0, 0, 0, None, wp.lat, wp.lon, wp.alt_m],
                    "doJumpId": wp.seq + 1,
                }
            )
        return {
            "fileType": "Plan",
            "geoFence": {"circles": [], "polygons": [], "version": 2},
            "groundStation": "CaesarAST",
            "mission": {
                "cruiseSpeed": 5,
                "firmwareType": 3,  # ArduPilot
                "globalPlanAltitudeMode": 1,
                "hoverSpeed": 5,
                "items": items,
                "plannedHomePosition": [home_lat, home_lon, 0],
                "vehicleType": 2,  # MAV_TYPE_QUADROTOR
                "version": 2,
            },
            "rallyPoints": {"points": [], "version": 2},
            "version": 1,
        }

from pydantic import BaseModel, Field, field_validator
from typing import List, Optional, Tuple, Literal

class TrackObservation(BaseModel):
    track_id: str = Field(..., pattern=r"^[\w.\-]{1,64}$")
    timestamp_ms: int
    modality: str
    confidence: float = Field(..., ge=0.0, le=1.0)
    class_label: str
    position_m: Tuple[float, float]
    velocity_mps: Optional[List[float]] = None
    source_id: str = Field(..., pattern=r"^[\w.\-]{1,64}$")
    evidence_digest: str = ""
    inference_latency_ms: Optional[int] = None
    inference_engine: str = ""

class PTZCommand(BaseModel):
    command_type: Literal["ptz-activate", "ptz-slew"]
    camera_id: str = Field(..., pattern=r"^[\w.\-]{1,64}$")
    target_position_m: Tuple[float, float]
    priority: int = Field(default=1, ge=1, le=10)

class DroneDispatchCommand(BaseModel):
    command_type: Literal["dispatch-drone"]
    drone_id: str = Field(..., pattern=r"^[\w.\-]{1,64}$")
    target_zone: str
    waypoints: List[Tuple[float, float]] = Field(..., max_length=50)
    priority: int = Field(default=1, ge=1, le=10)

    @field_validator('waypoints')
    @classmethod
    def validate_waypoints(cls, v):
        for lat, lon in v:
            if not (-90.0 <= lat <= 90.0) or not (-180.0 <= lon <= 180.0):
                raise ValueError(f"Invalid waypoint coordinates: {lat}, {lon}")
        return v

class ControlPayload(BaseModel):
    payload_id: str = Field(..., pattern=r"^[0-9a-fA-F\-]{36}$")
    timestamp_ms: int
    threat_level: Literal["monitor", "high-interest", "low", "medium", "high", "critical"]
    ptz_commands: List[PTZCommand] = Field(default_factory=list)
    drone_commands: List[DroneDispatchCommand] = Field(default_factory=list)

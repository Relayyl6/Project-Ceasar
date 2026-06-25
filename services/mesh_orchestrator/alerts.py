import os
import time
import logging

logger = logging.getLogger(__name__)

_PLACEHOLDER_MARKERS = ("your_", "example", "1234567890", "0987654321", "placeholder")


class AgenticAlerter:
    """Production Twilio voice-call alerter.

    On CRITICAL threat detection, dials AUTHORITIES_PHONE and plays a
    Text-to-Speech emergency message via the Twilio Programmable Voice API.
    Falls back to console simulation if credentials are not configured.
    """

    def __init__(self):
        self.account_sid = os.getenv("TWILIO_ACCOUNT_SID", "")
        self.auth_token  = os.getenv("TWILIO_AUTH_TOKEN", "")
        self.phone_from  = os.getenv("TWILIO_PHONE_FROM", "")
        self.authorities_phone = os.getenv("AUTHORITIES_PHONE", "")

        self._live = self._credentials_valid()
        if self._live:
            logger.info(
                "[alerts] Twilio LIVE mode active — will call %s on CRITICAL threats.",
                self.authorities_phone,
            )
        else:
            logger.warning(
                "[alerts] Twilio credentials missing or contain placeholders. "
                "Authority calls will be SIMULATED. Configure TWILIO_* env vars to enable live calls."
            )

        self.last_called_ms: int = 0
        self.cooldown_ms: int = 300_000  # 5-minute rate limit

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def trigger_authority_call(self, threat_type: str, lat: float, lon: float) -> bool:
        """Initiate an automated TTS voice call to authorities.

        Returns True if a call was initiated (or simulated), False if rate-limited.
        """
        now_ms = int(time.time() * 1000)
        remaining = self.cooldown_ms - (now_ms - self.last_called_ms)
        if remaining > 0:
            logger.info(
                "[alerts] Rate limit active (%ds remaining). Suppressing duplicate call for %s.",
                remaining // 1000, threat_type,
            )
            return False

        self.last_called_ms = now_ms

        twiml = self._build_twiml(threat_type, lat, lon)

        if self._live:
            return self._make_live_call(twiml, threat_type, lat, lon)
        else:
            self._simulate_call(twiml, threat_type, lat, lon)
            return True

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _credentials_valid(self) -> bool:
        """Return True only if all four credentials look like real values."""
        for val in (self.account_sid, self.auth_token, self.phone_from, self.authorities_phone):
            if not val:
                return False
            lower = val.lower()
            if any(marker in lower for marker in _PLACEHOLDER_MARKERS):
                return False
        return True

    def _build_twiml(self, threat_type: str, lat: float, lon: float) -> str:
        safe_type = threat_type.replace("&", "and").replace("<", "").replace(">", "")
        return (
            "<Response><Say voice='Polly.Matthew'>"
            "Project Caesar Emergency Alert. "
            f"Critical threat confirmed: {safe_type.replace('-', ' ')}. "
            f"Location: latitude {lat:.4f}, longitude {lon:.4f}. "
            "Autonomous response systems have been activated. "
            "Please dispatch units immediately."
            "</Say></Response>"
        )

    def _make_live_call(self, twiml: str, threat_type: str, lat: float, lon: float) -> bool:
        """POST to Twilio REST API to initiate the voice call."""
        import requests  # imported lazily to keep startup fast in simulation mode
        url = f"https://api.twilio.com/2010-04-01/Accounts/{self.account_sid}/Calls.json"
        try:
            resp = requests.post(
                url,
                auth=(self.account_sid, self.auth_token),
                data={"To": self.authorities_phone, "From": self.phone_from, "Twiml": twiml},
                timeout=10,
            )
            if resp.status_code in (200, 201):
                sid = resp.json().get("sid", "unknown")
                logger.info(
                    "[alerts] \u2705 AUTHORITY CALL INITIATED — SID: %s | %s | (%.6f, %.6f)",
                    sid, threat_type, lat, lon,
                )
                return True
            logger.error("[alerts] Twilio API error %d: %s", resp.status_code, resp.text[:300])
        except Exception as exc:
            logger.error("[alerts] Twilio request failed: %s", exc)
        # Fallback: at least log it locally
        self._simulate_call(twiml, threat_type, lat, lon)
        return False

    def _simulate_call(self, twiml: str, threat_type: str, lat: float, lon: float) -> None:
        """Console simulation for when Twilio is not configured."""
        bar = "=" * 80
        print(f"\n{bar}")
        print("\U0001f6a8  [CAESAR ALERT — SIMULATION MODE]  \U0001f6a8")
        print(f"  Threat      : {threat_type}")
        print(f"  Location    : lat={lat:.6f}  lon={lon:.6f}")
        print(f"  Would call  : {self.authorities_phone or 'NOT CONFIGURED'}")
        print(f"  TwiML       : {twiml[:120]}...")
        print("  To enable live calls — configure TWILIO_* vars in .env (see .env.example)")
        print(f"{bar}\n")

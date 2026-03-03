from datetime import datetime, timezone
from typing import Optional


def parse_vatsim_logon_time(logon_time_str: Optional[str]) -> datetime:

    # If the feed does not include a logon time, return current UTC time as a safe fallback
    # so callers don't fail; callers can detect "just now" if needed.
    if not logon_time_str:
        return datetime.now(timezone.utc)

    parts = logon_time_str.replace('Z', '').split('.')
    main_part = parts[0]

    if len(parts) > 1:
        fractional_seconds = parts[1]
        truncated_fractional = fractional_seconds[:6]
        logon_time_str_corrected = f"{main_part}.{truncated_fractional}+00:00"
    else:
        logon_time_str_corrected = f"{main_part}+00:00"

    return datetime.fromisoformat(logon_time_str_corrected)

def is_controller_active(controller: dict) -> bool:
    """Return True if the given controller entry represents an active connection.

    The data feed can represent activity in different places depending on version:
    - top-level `isActive`
    - `vatsimData.isActive`
    - any entry in `connections` with `isActive` True
    - any `positions` entry with `isActive` True
    """
    try:
        if controller.get("isActive"):
            return True

        vatsim = controller.get("vatsimData") or {}
        if vatsim.get("isActive"):
            return True

        for conn in controller.get("connections", []):
            if conn and conn.get("isActive"):
                return True

        for pos in controller.get("positions", []):
            if pos and pos.get("isActive"):
                return True

    except Exception:
        # If anything unexpected occurs, err on the side of not treating the
        # controller as active to avoid false-positives.
        return False

    return False

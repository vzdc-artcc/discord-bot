from typing import Optional, Any
import discord


def _extract_numeric(val: Any) -> Optional[float]:
    """
    Try to extract a float from a variety of types (int/float, numeric string,
    dict containing common keys, or a list/tuple where the first element can be parsed).
    Returns None if extraction fails.
    """
    if val is None:
        return None
    if isinstance(val, (int, float)):
        return float(val)
    if isinstance(val, str):
        try:
            return float(val)
        except Exception:
            return None
    if isinstance(val, dict):
        for k in ("value", "qnh", "altim", "alt", "hpa", "pressure"):
            if k in val and val[k] is not None:
                n = _extract_numeric(val[k])
                if n is not None:
                    return n
        # fallback: try any dict value
        for v in val.values():
            n = _extract_numeric(v)
            if n is not None:
                return n
        return None
    if isinstance(val, (list, tuple)) and val:
        # try first element then others
        for item in val:
            n = _extract_numeric(item)
            if n is not None:
                return n
        return None
    return None


def qnh_hpa_to_inhg(qnh_hpa: Any, *, round_digits: int | None = 2) -> Optional[float]:
    """
    Convert QNH from hPa (millibar) to inches of mercury (inHg).

    Accepts numeric, numeric string, dict or list containing a numeric value.

    Returns a rounded float in inHg or None if unable to parse.
    """
    numeric = _extract_numeric(qnh_hpa)
    if numeric is None:
        return None
    INHG_PER_HPA = 0.029529983071445
    inhg = numeric * INHG_PER_HPA
    return round(inhg, round_digits) if round_digits is not None else inhg


_CATEGORY_COLOR_MAP = {
    "VFR": discord.Color.green(),
    "MVFR": discord.Color.blue(),
    "IFR": discord.Color.red(),
    # use purple to represent very low IFR rather than a non-existent 'pink'
    "LIFR": discord.Color.purple(),
}


def get_category_color(category: Optional[str]) -> discord.Color:
    """
    Return a discord.Color for a flight category string (VFR, MVFR, IFR, LIFR).
    Case-insensitive. Returns dark grey for unknown/None.
    """
    if not category:
        return discord.Color.dark_grey()
    return _CATEGORY_COLOR_MAP.get(str(category).upper().strip(), discord.Color.dark_grey())

from typing import Optional
import discord

def qnh_hpa_to_inhg(qnh_hpa: float, *, round_digits: int | None = 2) -> float:
    """
    Convert QNH from hPa (millibar) to inches of mercury (inHg).

    Args:
        qnh_hpa: pressure in hPa (millibars).
        round_digits: number of decimal places to round to, or None to return full precision.

    Returns:
        Pressure in inHg.
    """
    INHG_PER_HPA = 0.029529983071445
    inhg = qnh_hpa * INHG_PER_HPA
    return round(inhg, round_digits) if round_digits is not None else inhg

_CATEGORY_COLOR_MAP = {
    "VFR": discord.Color.green(),
    "MVFR": discord.Color.blue(),
    "IFR": discord.Color.red(),
    "LIFR": discord.Color.pink(),
}

def get_category_color(category: Optional[str]) -> discord.Color:
    """
    Return a discord.Color for a flight category string (VFR, MVFR, IFR, LIFR).
    Case-insensitive. Returns dark grey for unknown/None.
    """
    if not category:
        return discord.Color.dark_grey()
    return _CATEGORY_COLOR_MAP.get(category.upper().strip(), discord.Color.dark_grey())
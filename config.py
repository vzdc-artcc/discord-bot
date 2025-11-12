import os
from dotenv import load_dotenv

load_dotenv()

# Gerneral
DISCORD_TOKEN = os.getenv("DISCORD_TOKEN")

# VATUSA
VATUSA_API_KEY = os.getenv("VATUSA_API_KEY")
VATUSA_API_URL = os.getenv("VATUSA_API_URL")

# Channels

STAFFUP_CHANNEL = int(os.getenv("STAFFUP_CHANNEL"))
BREAK_BOARD_CHANNEL_ID = int(os.getenv("BREAK_BOARD_CHANNEL_ID"))

# Roles

GND_UNRESTRICTED_ROLE_ID = int(os.getenv("GND_UNRESTRICTED_ROLE_ID"))
GND_TIER1_ROLE_ID = int(os.getenv("GND_TIER1_ROLE_ID"))
TWR_UNRESTRICTED_ROLE_ID = int(os.getenv("TWR_UNRESTRICTED_ROLE_ID"))
TWR_TIER1_ROLE_ID = int(os.getenv("TWR_TIER1_ROLE_ID"))
APP_UNRESTRICTED_ROLE_ID = int(os.getenv("APP_UNRESTRICTED_ROLE_ID"))
PCT_ROLE_ID = int(os.getenv("PCT_ROLE_ID"))
CENTER_ROLE_ID = int(os.getenv("CENTER_ROLE_ID"))


# Mapings
BREAK_BOARD_ROLE_MAP = {
    "gnd_unrestricted": GND_UNRESTRICTED_ROLE_ID,
    "gnd_tier1": GND_TIER1_ROLE_ID,
    "twr_unrestricted": TWR_UNRESTRICTED_ROLE_ID,
    "twr_tier1": TWR_TIER1_ROLE_ID,
    "app_unrestricted": APP_UNRESTRICTED_ROLE_ID,
    "pct": PCT_ROLE_ID,
    "center": CENTER_ROLE_ID,
}
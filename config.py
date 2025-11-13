import os
import discord
from dotenv import load_dotenv

load_dotenv()

# Gerneral
DISCORD_TOKEN = os.getenv("DISCORD_TOKEN")

# API
API_SECRET_KEY = os.getenv("API_SECRET_KEY")
API_PORT = int(os.getenv("API_PORT", 6000))

# VATUSA
VATUSA_API_KEY = os.getenv("VATUSA_API_KEY")
VATUSA_API_URL = os.getenv("VATUSA_API_URL")

# Channels
STAFFUP_CHANNEL = int(os.getenv("STAFFUP_CHANNEL"))
BREAK_BOARD_CHANNEL_ID = int(os.getenv("BREAK_BOARD_CHANNEL_ID"))
IMPROMPTU_CHANNEL_ID= int(os.getenv("IMPROMPTU_CHANNEL_ID"))

GENERAL_ANNOUNCEMENT_CHANNEL_ID = int(os.getenv("GENERAL_ANNOUNCEMENT_CHANNEL_ID"))
EVENT_ANNOUNCEMENT_CHANNEL_ID = int(os.getenv("EVENT_ANNOUNCEMENT_CHANNEL_ID"))
WEBSYSTEM_ANNOUNCEMENT_CHANNEL_ID = int(os.getenv("WEBSYSTEM_ANNOUNCEMENT_CHANNEL_ID"))
TRAINING_ANNOUNCEMENT_CHANNEL_ID = int(os.getenv("TRAINING_ANNOUNCEMENT_CHANNEL_ID"))
FACILITY_ANNOUNCEMENT_CHANNEL_ID = int(os.getenv("FACILITY_ANNOUNCEMENT_CHANNEL_ID"))

# Roles
GND_UNRESTRICTED_ROLE_ID = int(os.getenv("GND_UNRESTRICTED_ROLE_ID"))
GND_TIER1_ROLE_ID = int(os.getenv("GND_TIER1_ROLE_ID"))
TWR_UNRESTRICTED_ROLE_ID = int(os.getenv("TWR_UNRESTRICTED_ROLE_ID"))
TWR_TIER1_ROLE_ID = int(os.getenv("TWR_TIER1_ROLE_ID"))
APP_UNRESTRICTED_ROLE_ID = int(os.getenv("APP_UNRESTRICTED_ROLE_ID"))
PCT_ROLE_ID = int(os.getenv("PCT_ROLE_ID"))
CENTER_ROLE_ID = int(os.getenv("CENTER_ROLE_ID"))

IMPROMPTU_CTR_ROLE_ID = int(os.getenv("IMPROMPTU_CTR_ROLE_ID"))
IMPROMPTU_APP_ROLE_ID = int(os.getenv("IMPROMPTU_APP_ROLE_ID"))
IMPROMPTU_TWR_ROLE_ID = int(os.getenv("IMPROMPTU_TWR_ROLE_ID"))
IMPROMPTU_GND_ROLE_ID = int(os.getenv("IMPROMPTU_GND_ROLE_ID"))

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

IMPROMPTU_ROLE_MAP = {
    "impromptu_ctr": IMPROMPTU_CTR_ROLE_ID,
    "impromptu_app": IMPROMPTU_APP_ROLE_ID,
    "impromptu_twr": IMPROMPTU_TWR_ROLE_ID,
    "impromptu_gnd": IMPROMPTU_GND_ROLE_ID,
}

ANNOUNCEMENT_TYPES = {
    # Announcements
    "general": {
        "channel_id": GENERAL_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.blue().value,
        "title_prefix": "📢 General Announcement:"
    },
    "event": {
        "channel_id": EVENT_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.gold().value,
        "title_prefix": "🗓️ Event Announcement:"
    },
    "training": {
        "channel_id": TRAINING_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.green().value,
        "title_prefix": "🎓 Training Announcement:"
    },
    "websystem": {
        "channel_id": WEBSYSTEM_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.orange().value,
        "title_prefix": "🌐 Web System Announcement:"
    },
    "facility": {
        "channel_id": FACILITY_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.dark_teal().value,
        "title_prefix": "🏢 Facility Announcement:"
    },
    # Updates
    "general-update": {
        "channel_id": GENERAL_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.light_grey().value,
        "title_prefix": "⚙️ General Update:"
    },
    "event-update": {
        "channel_id": EVENT_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.dark_gold().value,
        "title_prefix": "🗓️ Event Update:"
    },
    "training-update": {
        "channel_id": TRAINING_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.dark_green().value,
        "title_prefix": "📚 Training Update:"
    },
    "websystem-update": {
        "channel_id": WEBSYSTEM_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.dark_orange().value,
        "title_prefix": "🛠️ Web System Update:"
    },
    "facility-update": {
        "channel_id": FACILITY_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.dark_grey().value,
        "title_prefix": "📰 Facility Update:"
    },
    "event-reminder": {
        "channel_id": EVENT_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.magenta().value,
        "title_prefix": "🔔 Event Reminder:"
    },
    "event-posting": {
        "channel_id": EVENT_ANNOUNCEMENT_CHANNEL_ID,
        "color": discord.Color.dark_blue().value,
        "title_prefix": "✨ New Event Posting:"
    },

}

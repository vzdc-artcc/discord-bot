import os

import discord
from dotenv import load_dotenv

load_dotenv()

# Gerneral
DISCORD_TOKEN = os.getenv("DISCORD_TOKEN")
VATUSA_API_KEY = os.getenv("VATUSA_TOKEN")

# API
API_SECRET_KEY = os.getenv("API_SECRET_KEY")
API_PORT = int(os.getenv("API_PORT", 5500))

# Announcment Channels
GENERAL_ANNOUNCEMENT_CHANNEL_ID = int(os.getenv("GENERAL_ANNOUNCEMENT_CHANNEL_ID"))
EVENT_ANNOUNCEMENT_CHANNEL_ID = int(os.getenv("EVENT_ANNOUNCEMENT_CHANNEL_ID"))
WEBSYSTEM_ANNOUNCEMENT_CHANNEL_ID = int(os.getenv("WEBSYSTEM_ANNOUNCEMENT_CHANNEL_ID"))
TRAINING_ANNOUNCEMENT_CHANNEL_ID = int(os.getenv("TRAINING_ANNOUNCEMENT_CHANNEL_ID"))
FACILITY_ANNOUNCEMENT_CHANNEL_ID = int(os.getenv("FACILITY_ANNOUNCEMENT_CHANNEL_ID"))

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

# Staff Up
STAFFUP_CHANNEL=int(os.getenv("STAFFUP_CHANNEL_ID"))

watched_positions = ['DCA_', 'IAD_', 'BWI_', 'PCT_', 'ADW_', 'RIC_', 'ROA_', 'ORF_', 'ACY_', 'NGU_',
                    'NTU_', 'NHK_', 'RDU_', 'CHO_', 'HGR_', 'LYH_', 'EWN_', 'LWB_', 'ISO_', 'MTN_', 'HEF_',
                    'MRB_', 'PHF_', 'SBY_', 'NUI_', 'FAY_', 'ILM_', 'NKT_', 'NCA_', 'NYG_', 'DAA_', 'DOV_',
                    'POB_', 'GSB_', 'WAL_', 'CVN_', 'JYO_', 'DC_']

# Break Board
BREAK_BOARD_CHANNEL_ID = int(os.getenv("BREAK_BOARD_CHANNEL_ID"))

GND_UNRESTRICTED_ROLE_ID = int(os.getenv("GND_UNRESTRICTED_ROLE_ID"))
GND_TIER1_ROLE_ID = int(os.getenv("GND_TIER1_ROLE_ID"))
TWR_UNRESTRICTED_ROLE_ID = int(os.getenv("TWR_UNRESTRICTED_ROLE_ID"))
TWR_TIER1_ROLE_ID = int(os.getenv("TWR_TIER1_ROLE_ID"))
APP_UNRESTRICTED_ROLE_ID = int(os.getenv("APP_UNRESTRICTED_ROLE_ID"))
PCT_ROLE_ID = int(os.getenv("PCT_ROLE_ID"))
CENTER_ROLE_ID = int(os.getenv("CENTER_ROLE_ID"))

BREAK_BOARD_ROLE_MAP = {
    "gnd_unrestricted": GND_UNRESTRICTED_ROLE_ID,
    "gnd_tier1": GND_TIER1_ROLE_ID,
    "twr_unrestricted": TWR_UNRESTRICTED_ROLE_ID,
    "twr_tier1": TWR_TIER1_ROLE_ID,
    "app_unrestricted": APP_UNRESTRICTED_ROLE_ID,
    "pct": PCT_ROLE_ID,
    "center": CENTER_ROLE_ID,
}

# Impromptu
IMPROMPTU_CHANNEL_ID = int(os.getenv("IMPROMPTU_CHANNEL_ID"))

IMPROMPTU_CTR_ROLE_ID = int(os.getenv("IMPROMPTU_CTR_ROLE_ID"))
IMPROMPTU_APP_ROLE_ID = int(os.getenv("IMPROMPTU_APP_ROLE_ID"))
IMPROMPTU_TWR_ROLE_ID = int(os.getenv("IMPROMPTU_TWR_ROLE_ID"))
IMPROMPTU_GND_ROLE_ID = int(os.getenv("IMPROMPTU_GND_ROLE_ID"))

IMPROMPTU_ROLE_MAP = {
    "impromptu_ctr": IMPROMPTU_CTR_ROLE_ID,
    "impromptu_app": IMPROMPTU_APP_ROLE_ID,
    "impromptu_twr": IMPROMPTU_TWR_ROLE_ID,
    "impromptu_gnd": IMPROMPTU_GND_ROLE_ID,
}

# Events
IMAGE_BASE_URL = os.getenv("IMAGE_BASE_URL")

NTML_CHANNEL_ID = int(os.getenv("NTML_CHANNEL_ID"))

STAFF_ROLE_ID = int(os.getenv("STAFF_ROLE_ID"))

# Rating Maps
atc_rating = {
    -1: 'INA', 0: 'SUS', 1: 'OBS', 2: 'S1', 3: 'S2', 4: 'S3', 5: 'C1', 6: 'C2', 7: 'C3',
    8: 'I1', 9: 'I2', 10: 'I3', 11: 'SUP', 12: 'ADM'
}

pilot_rating = {
    -1: 'INA', 0: 'P0', 1: 'PPL', 3: 'IR', 7: 'CMEL', 15: 'ATPL', 31: 'FI', 63: 'FE'
}

military_rating = {
    0: 'M0', 1: 'M1', 3: 'M2', 7: 'M3', 15: 'M4'
}

facility = {
    0: "OBS", 1: "FSS", 2: "DEL", 3: "GND", 4: "TWR", 5: "APP", 6: "CTR"
}

# Training

TRAINING_CATEGORY_ID = int(os.getenv("TRAINING_CATEGORY_ID"))
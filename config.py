import os
import json
import pathlib
import discord
import shutil
import time
import logging
import ast
from dotenv import load_dotenv

load_dotenv()

# --------- Environment-only secrets (stay in .env) ---------
DISCORD_TOKEN = os.getenv("DISCORD_TOKEN")

# API
API_SECRET_KEY = os.getenv("API_SECRET_KEY")
API_PORT = int(os.getenv("API_PORT", 6000))

# VATUSA
VATUSA_API_KEY = os.getenv("VATUSA_API_KEY")
VATUSA_API_URL = os.getenv("VATUSA_API_URL")

# Where per-guild configs are stored
MAIN_DIRECTORY = os.getcwd()
GUILD_CONFIG_FILE = os.getenv("GUILD_CONFIG_FILE", os.path.join(MAIN_DIRECTORY, "data", "guild_configs.json"))

# Internal cache for loaded guild configs
_guild_configs = {}

# Default shape for a guild config — keeps channel & role ids here
_DEFAULT_GUILD_CONFIG = {
    "channels": {
        "staffup_channel": None,
        "break_board_channel_id": None,
        "impromptu_channel_id": None,
        "general_announcement_channel_id": None,
        "event_announcement_channel_id": None,
        "websystem_announcement_channel_id": None,
        "training_announcement_channel_id": None,
        "facility_announcement_channel_id": None,
        "welcome_channel_id": None,
        "logging_channel_id": None,
    },
    "roles": {
        "gnd_unrestricted": None,
        "gnd_tier1": None,
        "twr_unrestricted": None,
        "twr_tier1": None,
        "app_unrestricted": None,
        "pct": None,
        "center": None,
        "impromptu_ctr": None,
        "impromptu_app": None,
        "impromptu_twr": None,
        "impromptu_gnd": None,
    },
    # Optional announcement overrides per guild
    "announcement_types": {}
}


class GuildConfig:
    """Simple wrapper around a per-guild config dictionary.

    Provides safe accessors for channels/roles and a method to resolve announcement
    type configuration falling back to module-level defaults.
    """

    def __init__(self, guild_id: int, data: dict):
        self.guild_id = int(guild_id)
        # Merge with defaults to ensure keys exist
        base = json.loads(json.dumps(_DEFAULT_GUILD_CONFIG))
        base.update(data or {})
        # deep merge for nested dicts
        for k in ("channels", "roles"):
            if k in data:
                base[k].update(data.get(k, {}))
        base["announcement_types"].update((data.get("announcement_types") or {}))
        self._data = base

    def get_channel(self, key: str):
        val = self._data.get("channels", {}).get(key)
        return int(val) if val is not None else None

    def get_role(self, key: str):
        val = self._data.get("roles", {}).get(key)
        return int(val) if val is not None else None

    def get_announcement_type(self, name: str):
        return self._data.get("announcement_types", {}).get(name)

    def as_dict(self):
        return self._data


def _load_guild_configs_from_disk():
    global _guild_configs
    path = pathlib.Path(GUILD_CONFIG_FILE)
    logging.getLogger(__name__).info("Loading guild configs from %s", path)
    if not path.exists():
        # create a default empty file if missing
        path.parent.mkdir(parents=True, exist_ok=True)
        # Default data to seed the guild config file (valid JSON)
        text = {
                  "1441276520927596587": {
                    "channels": {
                      "staffup_channel": 1441298656434323518,
                      "break_board_channel_id": 1441295212369547295,
                      "impromptu_channel_id": 1441295141347528855,
                      "general_announcement_channel_id": 1441483030320513245,
                      "event_announcement_channel_id": 1441483088260497629,
                      "websystem_announcement_channel_id": 1441483312878059660,
                      "training_announcement_channel_id": 1441483149241618484,
                      "facility_announcement_channel_id": 1441483207878115358
                    },
                    "roles": {
                      "gnd_unrestricted": 1441545335964176606,
                      "gnd_tier1": 1441545532613857483,
                      "twr_unrestricted": 1441544668247494688,
                      "twr_tier1": 1441545565182628011,
                      "app_unrestricted": 1441544492686774473,
                      "pct": 1441545450007166986,
                      "center": 1441546087683854397,
                      "impromptu_ctr": 1441544061491351643,
                      "impromptu_app": 1441544025449566412,
                      "impromptu_twr": 1441543998153031761,
                      "impromptu_gnd": 1441543041633489048
                    },
                    "announcement_types": {}
                  },
                  "391802941486530561": {
                    "channels": {
                      "staffup_channel": 1023762177569603694,
                      "break_board_channel_id": 1336549138237751366,
                      "impromptu_channel_id": 816168082900779008,
                      "general_announcement_channel_id": 1438253307553517588,
                      "event_announcement_channel_id": 519379679807799306,
                      "websystem_announcement_channel_id": 1201299423423115284,
                      "training_announcement_channel_id": 1336412583254556672,
                      "facility_announcement_channel_id": 975774188935717004
                    },
                    "roles": {
                      "gnd_unrestricted": 1348472041069613068,
                      "gnd_tier1": 1348472943780429905,
                      "twr_unrestricted": 1348472717929615360,
                      "twr_tier1": 1348473059887288542,
                      "app_unrestricted": 1348472832363073577,
                      "pct": 1348473122248069140,
                      "center": 1348473192909508619,
                      "impromptu_ctr": 816170273619312661,
                      "impromptu_app": 816170152180580353,
                      "impromptu_twr": 816170084958470145,
                      "impromptu_gnd": 816169915945844736
                    },
                    "announcement_types": {}
                  }
        }

        # Write valid JSON to disk
        try:
            path.write_text(json.dumps(text, indent=2))
        except Exception:
            # If writing fails, propagate so callers can handle it
            raise

        # Populate in-memory _guild_configs with GuildConfig objects
        loaded = {}
        for gid_str, cfg in text.items():
            try:
                gid = int(gid_str)
                loaded[gid] = GuildConfig(gid, cfg)
            except Exception:
                # Skip malformed entries
                continue
        _guild_configs = loaded
        logging.getLogger(__name__).info("Created default guild config file and loaded %d guild(s)", len(_guild_configs))
        return

    try:
        raw_text = path.read_text()
        try:
            raw = json.loads(raw_text)
        except Exception as e_json:
            # Attempt to recover from a Python dict repr (common when other tooling wrote the file)
            logging.getLogger(__name__).warning("Failed to parse %s as JSON: %s; attempting Python literal_eval fallback", path, e_json)
            try:
                raw = ast.literal_eval(raw_text)
                # Normalise: write back valid JSON so future loads succeed
                try:
                    path.write_text(json.dumps(raw, indent=2))
                except Exception:
                    logging.getLogger(__name__).exception("Failed to rewrite %s as JSON after literal_eval recovery", path)
            except Exception as e_eval:
                logging.getLogger(__name__).exception("Failed to parse %s as JSON or Python literal: %s", path, e_eval)
                _guild_configs = {}
                return

        # Expecting top-level mapping: guild_id -> config
        loaded = {}
        for gid_str, cfg in raw.items():
            try:
                gid = int(gid_str)
                loaded[gid] = GuildConfig(gid, cfg)
            except Exception:
                # skip malformed keys
                continue
        _guild_configs = loaded
        logging.getLogger(__name__).info("Loaded %d guild config(s) from %s", len(_guild_configs), path)
    except Exception:
        _guild_configs = {}


# Load at import time (and allow reload later)
_load_guild_configs_from_disk()


def reload_guild_configs():
    _load_guild_configs_from_disk()


def get_guild_config(guild_id: int) -> GuildConfig:
    """Return a GuildConfig for the specified guild id.

    If no config is found for the guild, returns a GuildConfig with defaults
    (all channel/role getters return None).
    """
    if guild_id is None:
        return GuildConfig(0, {})
    gid = int(guild_id)
    cfg = _guild_configs.get(gid)
    if cfg is None:
        # return empty config object for unknown guilds
        return GuildConfig(gid, {})
    return cfg


# Module-level defaults for announcements (fallback values)
# Keep the old ANNOUNCEMENT_TYPES shape but allow per-guild overrides via GuildConfig
ANNOUNCEMENT_TYPES = {
    # Announcements
    "general": {
        # channel_id intentionally None; real channel comes from per-guild config
        "channel_key": "general_announcement_channel_id",
        "color": discord.Color.blue().value,
        "title_prefix": "📢 General Announcement:"
    },
    "event": {
        "channel_key": "event_announcement_channel_id",
        "color": discord.Color.gold().value,
        "title_prefix": "🗓️ Event Announcement:"
    },
    "event-posting": {
        "channel_key": "event_announcement_channel_id",
        "color": discord.Color.gold().value,
        "title_prefix": "🗓️ Event Announcement:"
    },
    "training": {
        "channel_key": "training_announcement_channel_id",
        "color": discord.Color.green().value,
        "title_prefix": "🎓 Training Announcement:"
    },
    "websystem": {
        "channel_key": "websystem_announcement_channel_id",
        "color": discord.Color.orange().value,
        "title_prefix": "🌐 Web System Announcement:"
    },
    "facility": {
        "channel_key": "facility_announcement_channel_id",
        "color": discord.Color.dark_teal().value,
        "title_prefix": "🏢 Facility Announcement:"
    },
    # Updates and other legacy keys map to same channel keys
    "general-update": {"channel_key": "general_announcement_channel_id", "color": discord.Color.light_grey().value, "title_prefix": "⚙️ General Update:"},
    "event-update": {"channel_key": "event_announcement_channel_id", "color": discord.Color.dark_gold().value, "title_prefix": "🗓️ Event Update:"},
    "training-update": {"channel_key": "training_announcement_channel_id", "color": discord.Color.dark_green().value, "title_prefix": "📚 Training Update:"},
    "websystem-update": {"channel_key": "websystem_announcement_channel_id", "color": discord.Color.dark_orange().value, "title_prefix": "🛠️ Web System Update:"},
    "facility-update": {"channel_key": "facility_announcement_channel_id", "color": discord.Color.dark_grey().value, "title_prefix": "📰 Facility Update:"},
    "event-reminder": {"channel_key": "event_announcement_channel_id", "color": discord.Color.magenta().value, "title_prefix": "🔔 Event Reminder:"},
    "event-position-posting": {"channel_key": "event_announcement_channel_id", "color": discord.Color.dark_blue().value, "title_prefix": "Event Posting Posting:"},
    "event-announcement": {"channel_key": "event_announcement_channel_id", "color": discord.Color.dark_blue().value, "title_prefix": "Event Announcement:"},
}


def resolve_announcement_target_channel(guild_id: int, message_type: str):
    """Return a channel id (int) for the given guild and message_type.

    Looks for per-guild override under guild_config['announcement_types'][message_type]['channel_id']
    or falls back to ANNOUNCEMENT_TYPES mapping which references a channel_key.
    """
    mt = message_type.lower()
    if mt not in ANNOUNCEMENT_TYPES:
        return None
    guild_cfg = get_guild_config(guild_id)
    # Check per-guild announcement overrides first
    per = guild_cfg.get_announcement_type(mt)
    if per and per.get("channel_id"):
        return int(per.get("channel_id"))
    # Fallback to mapped channel key
    key = ANNOUNCEMENT_TYPES[mt].get("channel_key")
    return guild_cfg.get_channel(key)


# Backwards compatibility helpers: code that used old module-level constants can call these
# e.g. config.get_channel_for_guild(guild_id, 'break_board_channel_id')

def get_channel_for_guild(guild_id: int, key: str):
    return get_guild_config(guild_id).get_channel(key)


def get_role_for_guild(guild_id: int, key: str):
    return get_guild_config(guild_id).get_role(key)


# If you need to programmatically update a guild config at runtime, you can call save_guild_config
def save_guild_config(guild_id: int, data: dict):
    path = pathlib.Path(GUILD_CONFIG_FILE)
    try:
        raw = json.loads(path.read_text()) if path.exists() else {}
    except Exception:
        raw = {}
    raw[str(int(guild_id))] = data

    # Create a timestamped backup of the existing file to guard against accidental data loss
    try:
        if path.exists():
            bak_name = f"{path.name}.bak.{int(time.time())}"
            bak_path = path.parent.joinpath(bak_name)
            shutil.copy2(path, bak_path)
    except Exception:
        # Don't fail the save if backup can't be created; continue to attempt save
        pass

    # Atomic write: write to a temp file then replace the original
    try:
        tmp_path = path.parent.joinpath(path.name + ".tmp")
        tmp_path.write_text(json.dumps(raw, indent=2))
        # Use replace/rename which is atomic on most OSes
        tmp_path.replace(path)
    except Exception:
        # If atomic replace fails, fall back to direct write
        try:
            path.write_text(json.dumps(raw, indent=2))
        except Exception:
            # At this point the write failed; leave things as-is and re-raise
            raise
    reload_guild_configs()


# End of file

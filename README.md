# Discord Bot

This repository contains a Discord bot used across multiple guilds (servers). It supports per-guild configuration for channels, roles, and announcement types so a single bot token can be used by several servers with different settings.

Key files and structure

- `bot.py` - main bot entrypoint.
- `config.py` - configuration loader; reads environment secrets and a `data/guild_configs.json` for per-guild settings.
- `extensions/` - bot cogs / extensions that implement functionality (breakboard, impromptu, staffup, etc.).
- `data/` - runtime data and per-guild persisted files (e.g. role selector message ids, notification message ids, `guild_configs.json`).
- `requirements.txt` / `pyproject.toml` - dependencies.

Per-guild configuration

`config.py` expects a JSON file at `data/guild_configs.json` (path can be overridden with the `GUILD_CONFIG_FILE` env var). The file maps guild IDs (as strings) to a configuration object with the following shape:

{
  "<guild_id>": {
    "channels": { ... },
    "roles": { ... },
    "announcement_types": { ... }
  }
}

The available channel keys are:
- `staffup_channel`
- `break_board_channel_id`
- `impromptu_channel_id`
- `general_announcement_channel_id`
- `event_announcement_channel_id`
- `websystem_announcement_channel_id`
- `training_announcement_channel_id`
- `facility_announcement_channel_id`

The available role keys are:
- `gnd_unrestricted`, `gnd_tier1`, `twr_unrestricted`, `twr_tier1`, `app_unrestricted`, `pct`, `center`
- `impromptu_ctr`, `impromptu_app`, `impromptu_twr`, `impromptu_gnd`

Example

A sample guild config for the guild `1274456840033407093` is already provided in `data/guild_configs.json`.

Keeping secrets safe

Keep API keys and the Discord token in environment variables (e.g. a `.env` file). Do not store tokens or secret API keys in `guild_configs.json`.

Running the bot (development)

1. Create a virtual environment and install dependencies:

```bash
python -m venv .venv
source .venv/bin/activate.fish
pip install -r requirements.txt
```

2. Create a `.env` with the required secrets (see `.env.example` or `config.py` variables):

- `DISCORD_TOKEN` - the bot token (keep secret)
- `API_SECRET_KEY` (optional)
- `VATUSA_API_KEY`, `VATUSA_API_URL` (optional)
- `GUILD_CONFIG_FILE` (optional) to override the default `data/guild_configs.json`

3. Run the bot:

```bash
python bot.py
```

Notes

- The bot stores per-guild message IDs under `data/` with filenames like `breakboard_selector_message_id_<guild_id>.json` and `notification_message_id_<guild_id>.json` so the same bot can operate in multiple guilds without message ID collisions.
- If you need to add a new guild, add its config to `data/guild_configs.json` following the structure above, or use the `save_guild_config` helper in `config.py` to write one programmatically.

Troubleshooting

- If the bot reports a failed login, verify `DISCORD_TOKEN` is set correctly in your environment.
- If the bot can't find channels or roles, ensure the IDs in `data/guild_configs.json` are correct and that the bot has the necessary permissions in that guild.

License

MIT


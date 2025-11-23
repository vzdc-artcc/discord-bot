vZDC Discord Bot — README

This repo contains a Discord bot with per-guild configuration for channel and role IDs.

Quick overview
- Secrets (tokens, API keys) stay in environment variables (.env).
- Guild-specific settings (channel IDs, role IDs, announcement overrides) live in a JSON file: `data/guild_configs.json`.
- Message persistence for UI items (breakboard, role selectors) is saved per-guild under `data/` as `*_message_id_<guild_id>.json`.

Per-guild configuration (data/guild_configs.json)
- Top-level keys are guild IDs (strings).
- Each guild object can contain `channels`, `roles`, and `announcement_types` mappings.

Example (snippet):

{
  "1274456840033407093": {
    "channels": {
      "staffup_channel": 1405290009212358777,
      "break_board_channel_id": 1405289922985988206,
      "impromptu_channel_id": 1405304886970679436,
      "general_announcement_channel_id": 1438253307553517588,
      "event_announcement_channel_id": 1438253370317209741
    },
    "roles": {
      "gnd_unrestricted": 1437948612549152925,
      "gnd_tier1": 1437949560495280262
    }
  }
}

Notes:
- Any keys omitted fall back to defaults (the bot will skip features if channels/roles aren’t configured).
- You can override announcement channels per type using `announcement_types` inside a guild entry.

Environment variables (keep these secret)
- DISCORD_TOKEN — required (bot token)
- API_SECRET_KEY — used by the internal HTTP API
- VATUSA_API_KEY, VATUSA_API_URL, etc.
- Optional: GUILD_CONFIG_FILE — path to per-guild JSON (defaults to `data/guild_configs.json`).

Security: rotate your bot token immediately if it has been exposed.

Run locally (fish shell example)

```bash
# create and activate a venv (fish)
python -m venv .venv
source .venv/bin/activate.fish
pip install -r requirements.txt

# create or edit data/guild_configs.json with your guild/channel/role IDs
# ensure .env contains DISCORD_TOKEN and other secrets
python bot.py
```

API endpoints
- The announcement endpoints accept either `channel_id` (explicit override) or `guild_id` (the server ID). When `guild_id` is provided the bot will resolve the configured channel for the message type in that guild.
- Use X-API-Key header with your API_SECRET_KEY when calling the internal API.

Testing announcements (example payload)

POST /announcements
Headers: X-API-Key: <API_SECRET_KEY>
Body (JSON):
{
  "message_type": "event",
  "title": "Test",
  "body": "Hello",
  "guild_id": 1274456840033407093
}

Admin & future improvements
- Consider adding an admin command to modify per-guild config from Discord.
- Consider moving per-guild storage to a small database for concurrency.

If you want, I can:
- Add a small admin command for in-guild config management, or
- Add a short example admin workflow and automated tests.



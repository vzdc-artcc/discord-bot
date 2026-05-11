# Runbooks

## Bot Fails Startup

Check:

- the process can create and write to `logs/`
- Discord token validity
- Osmium bearer token validity
- Osmium route permissions for the service account
- channel naming in the Osmium config bundle
- role naming in the Osmium config bundle for selector-style features such as `impromptu_` and `break_board_`

## Outbound Job Delivery Fails

Check:

- recent JSON logs under `logs/` for route, dedupe, and delivery outcome fields
- `BOT_API_SHARED_KEY` matches Osmium callback auth
- `/ready` reports `config_loaded: true`
- configured Discord channel IDs still exist
- the bot can fetch the referenced event and positions from Osmium

## Slash Commands Missing

Check:

- `BOT_COMMAND_GUILD_IDS` is set for the target test guild
- the bot can reach Discord
- `/register-commands` succeeds for an operator

## Break Board Missing Or Broken

Check:

- JSON logs for `request_message_id`, `guild_id`, `channel_id`, and persistence failures
- Osmium has exactly one logical channel named `break_board`
- Osmium has the expected `break_board_` roles
- all `break_board_` roles belong to the same guild as the `break_board` channel
- persisted files at `BREAK_BOARD_STATE_PATH` and `BREAK_BOARD_REQUESTS_PATH` are readable and valid JSON
- the bot has permission to post messages, manage roles, and delete messages in the target channel

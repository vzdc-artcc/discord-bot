# Configuration

## Required Environment Variables

- `DISCORD_TOKEN`
- `DISCORD_APPLICATION_ID`
- `BOT_BIND_ADDR`
- `BOT_API_SHARED_KEY`
- `OSMIUM_BASE_URL`
- `OSMIUM_BEARER_TOKEN`

## Optional Environment Variables

- `BOT_COMMAND_GUILD_IDS`
- `BOT_OPERATOR_ROLE_IDS`
- `BOT_OPERATOR_USER_IDS`
- `AUDIT_LOGGING_ENABLED`
- `AUDIT_INCLUDE_BOT_EVENTS`
- `AUDIT_FETCH_AUDIT_LOGS`
- `AUDIT_MAX_FIELD_CHARS`
- `STAFFUP_ENABLED`
- `STAFFUP_POLL_INTERVAL_SECS`
- `STAFFUP_BATCH_SIZE`
- `STAFFUP_CURSOR_PATH`
- `STAFFUP_ENVIRONMENT`
- `STAFFUP_ARTCC_ID`
- `IMPROMPTU_SELECTOR_STATE_PATH`
- `BREAK_BOARD_STATE_PATH`
- `BREAK_BOARD_REQUESTS_PATH`
- `RUST_LOG`

## Notes

- `BOT_COMMAND_GUILD_IDS` should be a comma-separated list of guild IDs for faster command iteration.
- Operator IDs gate admin slash commands.
- `BOT_API_SHARED_KEY` must match the secret Osmium uses for outbound job callbacks.
- `AUDIT_LOGGING_ENABLED` defaults to `true`.
- `AUDIT_INCLUDE_BOT_EVENTS` defaults to `false`.
- `AUDIT_FETCH_AUDIT_LOGS` defaults to `true`.
- `AUDIT_MAX_FIELD_CHARS` defaults to `900`.
- The logical Osmium channel name for audit delivery is `audit_log`.
- Reliable message edit content requires the Discord `MESSAGE CONTENT` privileged intent.
- Audit actor attribution requires the bot to have `View Audit Log`.
- `STAFFUP_CURSOR_PATH` defaults to `data/staffup_cursor.json`.
- `IMPROMPTU_SELECTOR_STATE_PATH` defaults to `data/impromptu_selector_message.json`.
- `BREAK_BOARD_STATE_PATH` defaults to `data/break_board_messages.json`.
- `BREAK_BOARD_REQUESTS_PATH` defaults to `data/break_board_requests.json`.
- The shared `vZDC` embed footer icon is derived automatically from the configured Discord bot user's avatar. If the bot user has no custom avatar, Discord's default avatar URL is used.

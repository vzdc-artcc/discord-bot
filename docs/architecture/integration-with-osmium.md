# Integration With Osmium

## Bot -> Osmium

The bot authenticates to Osmium with a bearer token and uses these routes:

- `GET /api/v1/auth/service-account/me`
- `GET /api/v1/admin/integrations/discord/configs`
- `GET /api/v1/events/{event_id}`
- `GET /api/v1/events/{event_id}/positions?page=1&page_size=200`

## Osmium -> Bot

Osmium delivers durable outbound jobs to the bot over authenticated HTTP:

- `POST /announcement`
- `POST /event_position_posting`
- `POST /role_sync`

Osmium also proxies live guild discovery from the bot so the website's Discord
configuration UI can populate guild/channel/role dropdowns from real data
instead of pasted snowflake ids:

- `GET /guilds` — guilds the bot is a member of (`{id, name}`)
- `GET /guilds/{guild_id}/discovery` — that guild's channels, categories, and roles

Authentication currently accepts:

- `X-API-Key: <BOT_API_SHARED_KEY>`
- `Authorization: Bearer <BOT_API_SHARED_KEY>`

## Channel Naming Conventions

The bot currently expects these logical channel names in the Osmium Discord config bundle:

- `announcements`
- `event_position_posting`
- `staffup`
- `impromptu_training`
- `break_board`
- `audit_log`

Those names are used to resolve live Discord channel IDs for outbound delivery.

The bot currently expects these logical role prefixes in the Osmium Discord config bundle:

- `impromptu_`
- `break_board_`

## Development Note

An `/osmium` directory may be present locally for reference while building this bot, but it is not a required repo component and must not be treated as a compile-time dependency.

# Slash Commands

## Implemented

- `/ping` - Check bot responsiveness
- `/health` - Show bot readiness and config sync state
- `/sync-config` - Refresh Discord configuration from Osmium
- `/validate-guild` - Validate configured guild, channels, roles, and categories
- `/register-commands` - Re-register slash commands in configured guilds
- `/post-announcement-preview` - Post an announcement preview to configured channels
- `/post-event-preview` - Post an event position preview using live Osmium event data

## Access Model

- Operator commands (`sync-config`, `validate-guild`, `register-commands`, `post-announcement-preview`, `post-event-preview`) are gated by configured operator user IDs or role IDs
- If no operator lists are configured, the bot allows command execution to simplify local development
- Unauthorized attempts receive a clear "You are not authorized to use this command." response

## Input Validation

Commands validate user input before processing:

| Field | Rules |
|-------|-------|
| `title` | Required, trimmed, non-empty, max 256 characters |
| `body` | Required, trimmed, non-empty, max 4000 characters |
| `details_url` | Optional, must be valid http or https URL |
| `event_id` | Required, trimmed, non-empty, max 128 characters |

Validation failures return a clear message describing the issue.

## Preview Commands

- Announcement preview posts to all channels configured as `announcements`
- Event preview fetches the live event and positions from Osmium, then posts to channels configured as `event_position_posting`
- Preview embeds use the same shared `vZDC` footer and Discord-native send timestamp as production delivery embeds
- Preview messages suppress @mentions to prevent accidental pings

## Response Format

- Success responses are concise, human-readable confirmations
- Validation failures describe the specific issue
- Internal errors return a generic message with details logged server-side
- Guild validation returns a readable summary instead of raw JSON

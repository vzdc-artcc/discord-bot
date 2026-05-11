# Slash Commands

## Implemented

- `/ping`
- `/health`
- `/sync-config`
- `/validate-guild`
- `/register-commands`
- `/post-announcement-preview`
- `/post-event-preview`

## Access Model

- admin commands are gated by configured operator user IDs or role IDs
- if no operator lists are configured, the bot currently allows command execution to simplify local development

## Preview Commands

- announcement preview posts to all channels configured as `announcements`
- event preview fetches the live event and positions from Osmium, then posts to channels configured as `event_position_posting`
- preview embeds use the same shared `vZDC` footer and Discord-native send timestamp as production delivery embeds

# Audit Log

The audit log feature posts Discord embeds to the logical `audit_log` channel from the Osmium Discord config bundle.

## Coverage

- message edit and delete
- bulk message delete
- channel create, update, and delete
- role create, update, and delete
- thread create, update, and delete
- member join, leave, and update
- ban and unban
- emoji and sticker set updates

## Behavior

- embeds use the shared `vZDC` footer with the bot avatar and Discord-native send timestamp
- message bodies are included when available and truncated to fit embed limits
- deleted message content is best-effort and depends on cache availability
- bot-authored actions are skipped by default unless `AUDIT_INCLUDE_BOT_EVENTS=true`
- guild administration events attempt best-effort actor attribution through Discord audit logs

## Configuration

- the logical channel name must be `audit_log`
- if `AUDIT_LOGGING_ENABLED=true` and the channel is missing, `/ready` reports not ready
- `AUDIT_FETCH_AUDIT_LOGS=true` enables actor enrichment for channel, role, thread, member moderation, ban, emoji, and sticker changes
- `AUDIT_MAX_FIELD_CHARS` controls per-field truncation

## Discord Requirements

- `MESSAGE CONTENT` privileged intent is required for reliable edited message content
- `View Audit Log` permission is required for actor attribution

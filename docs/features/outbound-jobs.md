# Outbound Jobs

## Supported Bot Endpoints

- `POST /announcement`
- `POST /event_position_posting`

## Behavior

- requests are authenticated with the shared callback key
- malformed or unauthorized requests return `4xx`
- real delivery failures return non-`2xx` so Osmium can retry
- repeated payloads are deduplicated in-memory for a short rolling window

## Current Payload Shapes

Announcement:

```json
{
  "title": "Training Freeze",
  "body_markdown": "Training is paused for maintenance tonight.",
  "details_url": "https://example.test/announcements/training-freeze",
  "requested_by_cid": 1234567
}
```

Event position posting:

```json
{
  "event_id": "event_uuid",
  "ping_users": true,
  "requested_by_cid": 1234567
}
```

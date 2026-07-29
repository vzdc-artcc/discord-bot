# Outbound Jobs

## Supported Bot Endpoints (extra)

- `POST /scheduled_event` — creates a native Discord scheduled event (server Events
  tab) in every configured guild from a website event's title/description/start/end
  (External type + a `location`). Re-running replaces the prior one (tracked in
  `SCHEDULED_EVENT_STATE_PATH`); cover image is best-effort from `EVENT_BANNER_BASE_URL`.

## Supported Bot Endpoints

- `POST /announcement`
- `POST /event_position_posting`

## Behavior

- requests are authenticated with the shared callback key
- malformed or unauthorized requests return `4xx`
- real delivery failures return non-`2xx` so Osmium can retry
- repeated payloads are deduplicated in-memory for a short rolling window
- the event-posting embed leads with a link back to the signup page
  (`EVENT_SIGNUP_BASE_URL` + `/events/{id}`, also the clickable title), then the
  event description, `Start`/`End` as Discord timestamps, an image from
  `EVENT_BANNER_BASE_URL` + the event banner, and positions grouped by facility
  (GND/TWR/APP/CTR/… from the callsign suffix, falling back to
  `controlling_category`) rendered as `@mention (rating) — callsign` (linked
  controllers show as name pills in the embed but embeds never notify)
- when `ping_users` is true, assigned controllers with a linked Discord id are
  notified via a **separate mention-only message that is deleted immediately** (a
  "ghost ping"), so the notification fires but only the posting embed remains

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

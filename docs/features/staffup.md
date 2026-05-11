# Staffup

The staffup worker polls Osmium `GET /api/v1/stats/controller-events` and posts Discord embeds for live controller position transitions.

## Startup Behavior

- on first startup with no cursor file, the worker bootstraps to the latest known controller event ID
- that bootstrap does not emit any historical Discord messages
- once a cursor exists, normal resume behavior uses the saved `last_event_id`
- if you want to reset the baseline manually, delete `STAFFUP_CURSOR_PATH` before restarting the bot

## Filtering

- environment must be `live`
- event type must be `position_activated` or `position_deactivated`
- payload `is_primary` must be `true`
- payload `artcc_id` must match `STAFFUP_ARTCC_ID`, default `ZDC`

## Discord Channel

The bot resolves a logical Discord channel named `staffup` from the Osmium Discord config bundle.

## Presentation

- online embeds are green
- offline embeds are red
- frequencies are normalized from Osmium raw values like `121900000` to VHF display like `121.900`
- controller names display rating in parentheses using `user_rating`, then `requested_rating`
- embeds use the shared `vZDC` footer with the Discord bot user's avatar icon and Discord-native send timestamp

## Persistence

The worker stores its cursor and open activations in `STAFFUP_CURSOR_PATH`, default `data/staffup_cursor.json`.

## Failure Modes

- missing `staffup` logical channel degrades readiness and pauses advancement
- Osmium polling failures keep the existing cursor and retry later
- Discord delivery failures stop advancement past the failed event

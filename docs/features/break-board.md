# Break Board

The break board feature maintains two persistent Discord embeds in the logical `break_board` channel from the Osmium Discord config bundle.

## Persistent Messages

At startup, the bot creates or refreshes:

- a notification preference embed for `break_board_` roles
- a break request embed that opens a modal for new requests

The bot stores those message IDs in `BREAK_BOARD_STATE_PATH`, default `data/break_board_messages.json`.

## Role And Channel Discovery

- channel name: `break_board`
- role prefix: `break_board_`

All `break_board_` roles must belong to the same guild as the `break_board` channel.

## Request Flow

1. A controller clicks a break request button for a role audience.
2. The bot opens a modal with:
   - required `time_before_close`
   - optional `position`
   - optional `notes`
3. The bot posts a request message in `break_board` that pings the selected role.
4. Another member may press `Claim`.
5. The bot marks the original request as claimed and posts a second handoff message tagging the requester and claimer.
6. The requester or claimer may press `Complete / Delete` to remove the request flow.

## Time Parsing

The modal accepts positive free-text durations such as:

- `5`
- `15m`
- `45 minutes`
- `1h`
- `1h 15m`

## Timeouts

- unclaimed requests auto-delete at `time before close + 5 minutes`
- claimed handoff flows auto-delete `5 minutes` after claim

The bot persists active request state in `BREAK_BOARD_REQUESTS_PATH`, default `data/break_board_requests.json`, so cleanup still works after restart.

## Authorization

- preference toggle buttons add or remove the corresponding role for the clicking member
- only the original requester may delete an unclaimed request
- the original requester cannot claim their own request
- either the original requester or the claimer may complete a claimed request

## Failure Modes

- missing `break_board` channel blocks startup refresh for the feature
- missing `break_board_` roles blocks startup refresh for the feature
- malformed persisted state files return configuration/runtime errors on load
- Discord API failures can leave orphaned request messages if posting succeeds but persistence fails, though the bot attempts cleanup in that case

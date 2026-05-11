# TODO

## Now

- [x] Keep the bot as a standalone root crate with clean boundaries to the external Osmium backend
- [x] Build the initial Serenity + Axum bot service skeleton
- [x] Add typed Osmium client integration and startup config sync
- [x] Add authenticated outbound job endpoints for announcements and event position posting
- [x] Add staffup worker polling Osmium controller events and posting live position transitions
- [x] Create the first-pass markdown docs set for architecture, setup, commands, and operations

## Next

- [ ] Tighten Discord message formatting for production presentation and mention policies
- [ ] Add richer command option validation and operator feedback
- [ ] Add contract fixtures for Osmium callback payloads and API responses
- [ ] Add CI workflow checks for `fmt`, `clippy`, and `test`
- [ ] Add deployment manifests and environment-specific config examples

## Later

- [ ] Add richer user-facing slash commands
- [ ] Expand Discord account-link operational tooling
- [ ] Add role sync and broader guild automation workflows
- [ ] Introduce shared crates only if pure DTO/helper duplication becomes persistent

## Done

- [x] Establish the separate-service, API-first runtime architecture
- [x] Document the bot/Osmium ownership boundary and callback contracts

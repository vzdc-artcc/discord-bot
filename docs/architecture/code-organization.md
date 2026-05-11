# Code Organization

## Bot Crate Structure

- `src/app.rs`: process assembly and startup flow
- `src/config.rs`: env parsing
- `src/logging.rs`: tracing initialization
- `src/state.rs`: shared runtime state and delivery dedupe
- `src/osmium/`: typed API client
- `src/http/`: internal callback server
- `src/discord/`: Serenity event handling
- `src/commands/`: slash command definitions and dispatch
- `src/services/`: delivery and guild validation logic
- `src/models/`: integration DTOs and runtime response types
- `src/errors.rs`: shared app error types

## Conventions

- Keep Discord HTTP and gateway concerns out of command parsing code.
- Keep Osmium API calls in the dedicated client module.
- Keep callback handlers thin; push delivery logic into services.
- Keep embed branding in the Discord service layer so footer text, icon, and timestamps are applied consistently in one place.
- Update docs and `TODO.md` when public integration behavior changes.

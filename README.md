# vZDC Discord Bot Workspace

This repository contains the Rust rewrite of the vZDC Discord bot at the repo root.

## Layout

- `src/`: the Serenity-based Discord bot service
- `docs/`: bot and integration documentation
- `TODO.md`: concise roadmap and implementation tracker

`/osmium` may exist locally as a reference copy during development, but it is not a required part of this repo and is not treated as a workspace member.

## Local Startup

Start your Osmium backend separately:

```bash
# run Osmium from its own repo or local reference copy
```

Run the bot from the workspace root:

```bash
cp .env.example .env
cargo run
```

The bot derives the shared `vZDC` embed footer icon from the configured Discord bot user's current avatar or default avatar.

The bot also supports Discord-side audit logging through an Osmium logical channel named `audit_log`. For reliable message edit content, enable the Discord `MESSAGE CONTENT` privileged intent. For actor attribution on guild administration changes, grant the bot `View Audit Log`.

## Logging

- console logs are human-readable text
- structured JSON logs are written to daily-rotated files under `logs/`
- `RUST_LOG` controls the shared filter for both outputs
- secrets and full request/message bodies are intentionally excluded from logs

## Integration Model

- Osmium owns data, durable outbound jobs, auth, and integration config as an external backend.
- The bot owns Discord connectivity, slash commands, message delivery, and guild-side validation.
- Osmium calls the bot over authenticated HTTP for outbound jobs.
- The bot calls Osmium over authenticated bearer-token APIs for config, event data, and staffup controller events.

See [docs/index.md](docs/index.md) for the full documentation set.

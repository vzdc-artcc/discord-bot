# Local Development

## Prerequisites

- Rust toolchain
- a Discord application and bot token
- an Osmium API key with the required integration permissions
- a running Osmium environment reachable over HTTP

## Start Osmium

```bash
# run Osmium from its own repository or another reachable environment
```

## Start The Bot

From the workspace root:

```bash
cargo run
```

If you want guild-scoped slash commands registered automatically, set `BOT_COMMAND_GUILD_IDS`.

## Common Validation Commands

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

# Deployment

The bot is designed to run as an independent service from Osmium.

## Requirements

- network access to Discord
- network access to the Osmium API
- a valid Discord bot token
- a valid Osmium bearer token
- a shared callback secret matching Osmium
- write access to the local `logs/` directory for daily-rotated JSON logs

## Logs

- console output remains human-readable for local operations
- JSON logs are written to daily-rotated files under `logs/`
- `RUST_LOG` controls verbosity for both console and file outputs
- secrets and full request bodies are not logged

## Health Endpoints

- `GET /health`
- `GET /ready`

`/ready` returns false until Discord has connected and the Osmium Discord config bundle has loaded successfully.

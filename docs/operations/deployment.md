# Deployment

The bot is designed to run as an independent service from Osmium.

## Requirements

- Network access to Discord
- Network access to the Osmium API
- A valid Discord bot token
- A valid Osmium bearer token
- A shared callback secret matching Osmium
- Write access to `data/` for state persistence
- Write access to `logs/` for daily-rotated JSON logs

## Docker Compose Deployment

The recommended deployment method uses Docker Compose.

### Quick Start

1. Copy the environment template and fill in secrets:

   ```bash
   cp .env.production.example .env
   # Edit .env with your credentials
   ```

2. Create data and logs directories:

   ```bash
   mkdir -p data logs
   ```

3. Build and start the service:

   ```bash
   docker compose up -d --build
   ```

4. Verify the service is running:

   ```bash
   docker compose logs -f bot
   curl http://localhost:3010/health
   curl http://localhost:3010/ready
   ```

### Configuration

Environment variables are loaded from `.env` by Docker Compose. See `.env.example` for all available options.

Required secrets:

| Variable | Description |
|----------|-------------|
| `DISCORD_TOKEN` | Bot token from Discord Developer Portal |
| `DISCORD_APPLICATION_ID` | Application ID from Discord Developer Portal |
| `BOT_API_SHARED_KEY` | Shared secret for Osmium callback authentication |
| `OSMIUM_BEARER_TOKEN` | Service account token for Osmium API |

Operator access control:

| Variable | Description |
|----------|-------------|
| `BOT_COMMAND_GUILD_IDS` | Comma-separated guild IDs where commands are registered |
| `BOT_OPERATOR_ROLE_IDS` | Comma-separated role IDs allowed to run operator commands |
| `BOT_OPERATOR_USER_IDS` | Comma-separated user IDs allowed to run operator commands |

### Volumes

| Host Path | Container Path | Purpose |
|-----------|----------------|---------|
| `./data` | `/app/data` | State persistence (staffup cursor, message states) |
| `./logs` | `/app/logs` | Daily-rotated JSON log files |

### Network

The bot requires outbound HTTPS access to:

- Discord API (`discord.com`, `gateway.discord.gg`)
- Osmium API (configured via `OSMIUM_BASE_URL`)

The bot exposes port 3010 for inbound HTTP callbacks from Osmium.

### Updating

```bash
docker compose pull
docker compose up -d --build
```

## Manual Deployment

For non-containerized environments:

1. Build the release binary:

   ```bash
   cargo build --release
   ```

2. Copy the binary and create directories:

   ```bash
   cp target/release/vzdc-discord-bot /opt/vzdc-bot/
   mkdir -p /opt/vzdc-bot/data /opt/vzdc-bot/logs
   ```

3. Create an environment file at `/opt/vzdc-bot/.env`

4. Run the service:

   ```bash
   cd /opt/vzdc-bot
   ./vzdc-discord-bot
   ```

Consider using systemd or another process manager for production deployments.

## Logs

- Console output remains human-readable for local operations
- JSON logs are written to daily-rotated files under `logs/`
- `RUST_LOG` controls verbosity for both console and file outputs
- Secrets and full request bodies are not logged

## Health Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Basic liveness check |
| `GET /ready` | Readiness check including Discord connection and config sync state |

`/ready` returns false until Discord has connected and the Osmium Discord config bundle has loaded successfully.

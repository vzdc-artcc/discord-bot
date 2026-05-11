# Architecture Overview

The vZDC Discord bot is a separate Rust service built with Serenity for Discord connectivity and Axum for authenticated internal callback endpoints.

## Ownership

- Osmium owns users, permissions, Discord integration config, event data, and durable outbound jobs through an external backend API.
- The bot owns Discord gateway connectivity, slash commands, message delivery, and guild validation.

## Runtime Shape

- one bot process
- one Discord gateway connection
- one internal HTTP listener
- one typed Osmium API client
- one in-memory config and readiness cache

## Why This Shape

This keeps the bot operationally independent while still tightly integrating with Osmium through explicit contracts.

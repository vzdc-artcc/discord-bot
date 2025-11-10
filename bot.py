import os
import discord
import asyncio
import atexit
import json
import pathlib
import logging.config
import logging.handlers

from config import DISCORD_TOKEN
from discord.ext import commands


intents = discord.Intents.all()
intents.messages = True
intents.members = True

bot = commands.Bot(command_prefix='!', intents=intents, case_insensitive=True)

logger = logging.getLogger(__name__)

def setup_logging():
    config_file = pathlib.Path("logging_conf.json")
    with open(config_file) as f_in:
        config = json.load(f_in)

    # Ensure any file handler target directories exist before configuring logging.
    handlers = config.get("handlers", {})
    for h_name, h_conf in handlers.items():
        filename = h_conf.get("filename")
        if filename:
            try:
                path = pathlib.Path(filename)
                if path.parent and not path.parent.exists():
                    path.parent.mkdir(parents=True, exist_ok=True)
            except Exception:
                # If directory creation fails for any reason, keep going and
                # let dictConfig raise a clear error instead of crashing here.
                pass

    logging.config.dictConfig(config)

    # Try to find a QueueHandler instance and start its attached listener if present.
    queue_handler = None
    for h in logging.root.handlers:
        if isinstance(h, logging.handlers.QueueHandler):
            queue_handler = h
            break

    if queue_handler is not None:
        listener = getattr(queue_handler, "listener", None)
        if listener is not None and hasattr(listener, "start"):
            listener.start()
            atexit.register(listener.stop)


@bot.event
async def on_command_error(ctx, error):
    # This will be called by discord.py when a command raises an error.
    if isinstance(error, commands.CommandNotFound):
        await ctx.send("Sorry, that command doesn't exist.")
    elif isinstance(error, commands.MissingPermissions):
        await ctx.send("You don't have the necessary permissions to run this command.")
    else:
        logger.error(f"Unhandled command error in {ctx.command}: {error}", exc_info=True)
        await ctx.send(f"An unexpected error occurred: `{error}`")


async def load_extensions():
    extensions_path = pathlib.Path("./extensions")
    if not extensions_path.exists():
        logger.warning("No extensions directory found at %s — skipping extension loading.", extensions_path)
        return
    if not extensions_path.is_dir():
        logger.warning("Extensions path %s exists but is not a directory — skipping extension loading.", extensions_path)
        return

    for filename in os.listdir(str(extensions_path)):
        if filename.endswith(".py"):
            extension_name = filename[:-3]
            try:
                await bot.load_extension(f"extensions.{extension_name}")
                logger.info(f"Loaded extension: {extension_name}")
            except Exception as e:
                logger.error(f"An unexpected error occurred while loading extension "
                             f"'{extension_name}': {e}", exc_info=True)


async def main():
    setup_logging()
    logger.info("Bot starting...")
    await load_extensions()

    try:
        await bot.start(DISCORD_TOKEN)
    except discord.LoginFailure:
        logger.critical("Failed to log in to Discord. Please check your token.")
    except Exception as e:
        logger.critical("An unhandled exception occurred during bot startup/runtime: %s", e, exc_info=True)
    finally:
        logger.info("Bot shutting down.") # Log shutdown


if __name__ == "__main__":
    asyncio.run(main())
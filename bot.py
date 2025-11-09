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

logger = logging.getLogger('__name__')

def setup_logging():
    config_file = pathlib.Path("logging_conf.json")
    with open(config_file) as f_in:
        config = json.load(f_in)

    logging.config.dictConfig(config)
    queue_handler = logging.getHandlerByName("queue_handler")
    if queue_handler is not None:
        queue_handler.listener.start()
        atexit.register(queue_handler.listener.stop)

async def on_command_error(ctx, error):
    if isinstance(error, commands.CommandNotFound):
        await ctx.send("Sorry, that command doesn't exist.")
    elif isinstance(error, commands.MissingPermissions):
        await ctx.send("You don't have the necessary permissions to run this command.")
    else:
        logger.error(f"Unhandled command error in {ctx.command}: {error}", exc_info=True)
        await ctx.send(f"An unexpected error occurred: `{error}`")


async def load_extensions():
    for filename in os.listdir("./extensions"):
        if filename.endswith(".py"):
            extension_name = filename[:-3]
            try:
                await bot.load_extension(f"extensions.{extension_name}")
                logger.info(f"Loaded extension: {extension_name}")
            except ExtensionLoadError as e:
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
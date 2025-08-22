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
        print(f"Unhandled command error in {ctx.command}: {error}")
        await ctx.send(f"An unexpected error occurred: `{error}`")


async def load_extensions():
    for filename in os.listdir("./extensions"):
        if filename.endswith(".py"):
            try:
                await bot.load_extension(f"extensions.{filename[:-3]}")
                print(f"Loaded extension: {filename[:-3]}")
            except Exception as e:
                print(f"Failed to load extension {filename[:-3]}: {e}")

async def main():
    setup_logging()
    await load_extensions()
    await bot.start(DISCORD_TOKEN)
    print("Bot Started")


if __name__ == "__main__":
    asyncio.run(main())
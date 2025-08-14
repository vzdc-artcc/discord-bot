# bot.py
import discord
from discord.ext import commands
from config import DISCORD_TOKEN
import asyncio
import os

intents = discord.Intents.all()
intents.message_content = True
intents.members = True

bot = commands.Bot(command_prefix='!',
                   intents=intents,
                   case_insensitive=True
                   )

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
    await load_extensions()
    await bot.start(DISCORD_TOKEN)
    print("Bot Started")


if __name__ == "__main__":
    asyncio.run(main())
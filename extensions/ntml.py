import discord
from discord.ext import commands
from config import NTML_CHANNEL_ID

class NTMLListener(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    @commands.Cog.listener()
    async def on_message(self, message):
        if message.channel.id == NTML_CHANNEL_ID and not message.author.bot:
            print(message.content)
            # async with aiohttp.ClientSession() as session:
            #     await session.post(EXTERNAL_URL, json=data)

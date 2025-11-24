import discord
from discord.ext import commands
from typing import Optional
from datetime import datetime, timezone

import config


class Welcome(commands.Cog):
    """Send a configurable welcome message to a specified channel when a member joins.

    Behavior:
    - Reads per-guild `welcome_channel_id` from config.get_channel_for_guild(guild_id, 'welcome_channel_id')
    - If present and resolvable, sends a formatted message mentioning the new member.
    - Silently does nothing if no channel is configured or message send fails.
    """

    def __init__(self, bot: commands.Bot, *, template: Optional[str] = None):
        self.bot = bot
        # Default template; can be overridden when constructing the cog
        self.template = template or (
            "Welcome {mention}!\n"
            "Welcome to the vZDC Discord. Thanks for being part of the community.\n\n"
            "Welcome to vZDC! We’re excited to have you join us as a controller in one of the busiest and most complex airspaces on VATSIM. You’re now part of a dedicated team of virtual controllers committed to excellence, professionalism, and supporting one another.\n\n"
            "Some stuff about rules etc:\n\n"
            "- shdauhd\n\n"
            "- dsadsad\n\n"
            "We’re thrilled to have you onboard. **Welcome home to the nation's capital!**"
        )

    @commands.Cog.listener()
    async def on_member_join(self, member: discord.Member):
        try:
            guild = member.guild
            if guild is None:
                return
            channel_id = config.get_channel_for_guild(guild.id, "welcome_channel_id")
            if not channel_id:
                return
            channel = guild.get_channel(int(channel_id))
            # guild.get_channel returns None for some channel types; fallback to bot.fetch_channel
            if channel is None:
                try:
                    channel = await self.bot.fetch_channel(int(channel_id))
                except Exception:
                    return

            # Build an embed welcome message
            embed = discord.Embed(
                title=f"Welcome to vZDC, {member.display_name}!",
                description=(
                    f"Welcome {member.mention}!\n\n"
                    "Welcome to the vZDC Discord. Thanks for being part of the community.\n\n"
                    "We’re excited to have you join us as a controller in one of the busiest and most complex airspaces on VATSIM. You’re now part of a dedicated team of virtual controllers committed to excellence, professionalism, and supporting one another.\n\n"
                    "Some stuff about rules etc:\n\n"
                    "- shdauhd\n"
                    "- dsadsad\n\n"
                    "We’re thrilled to have you onboard. **Welcome home to the nation's capital!**"
                ),
                color=discord.Color.red()
            )

            # Try to set the user's avatar as the thumbnail
            try:
                avatar_url = member.display_avatar.url
            except Exception:
                avatar_url = None
            if avatar_url:
                embed.set_thumbnail(url=avatar_url)

            # Add some helpful fields
            try:
                created = member.created_at.strftime("%Y-%m-%d %H:%M:%S UTC")
            except Exception:
                created = str(member.created_at)
            embed.add_field(name="Discord Account Created", value=created, inline=False)
            embed.timestamp = datetime.now(timezone.utc)
            embed.set_footer(text="vZDC", icon_url=guild.icon.url if guild.icon else None)

            await channel.send(embed=embed)
        except Exception:
            # Avoid crashing the bot on any unexpected failure
            logging = __import__("logging")
            logging.getLogger("server_events").exception("Failed to send welcome message")


async def setup(bot: commands.Bot):
    await bot.add_cog(Welcome(bot))

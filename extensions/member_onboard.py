import discord
import logging
from discord.ext import commands


class MemberOnboard(commands.Cog):
    """Send a direct message to users when they join the guild.

    - Skips bot accounts.
    - Handles `discord.Forbidden` when the member has DMs closed or has blocked the bot.
    - Logs successes and failures to the "server_events" logger (matches project logging).
    """

    def __init__(self, bot: commands.Bot):
        self.bot = bot
        self.logger = logging.getLogger("server_events")

    @commands.Cog.listener()
    async def on_member_join(self, member: discord.Member):
        # Ignore bot accounts
        if member.bot:
            return

        guild_name = member.guild.name if member.guild else "this server"

        dm_text = (
            f"Welcome to vZDC! We’re excited to have you join us as a controller in one of the busiest and most complex airspaces on VATSIM. You’re now part of a dedicated team of virtual controllers committed to excellence, professionalism, and supporting one another."
            "To get started:\n\n"
            "- **You are authorized to work any unrestricted position up to your current rating without further training.** *(Not applicable for OBS rated controllers)*\n\n"
            "- **Join our Discord server** by navigating to the **“Community”** section in the top navigation bar and selecting **“Discord”**.\n\n"
            "- **Review our publications**, including SOPs and reference materials, by heading to the **“Publications”** section in the top navigation bar.\n\n"
            "- **Familiarize yourself with our General Operating Policy and Training Policy** — both are essential to understanding how things run at ZDC.\n\n"
            "- **Request your first training assignment** by clicking your name in the top-right corner of the site, selecting your **profile**, and scrolling to the **“Assigned Trainers”** section. The training assignment notification will be sent via email.\n\n"
            "- If you have any questions, check out the **ARTCC Staff** page under the **“Controllers”** section in the top navigation bar. Our staff is here to help and happy to assist with anything you need.\n\n"
            "We’re thrilled to have you onboard. **Welcome home to the nation's capital!**"
        )

        try:
            await member.send(dm_text)
            self.logger.info("Sent welcome DM to %s (%s)", member, member.id)
        except discord.Forbidden:
            # The member has DMs closed or has blocked the bot
            self.logger.warning("Could not send DM to %s (%s): forbidden/DMs closed", member, member.id)
        except Exception:
            # Log unexpected failures but don't crash the bot
            self.logger.exception("Unexpected error sending DM to %s (%s)", member, member.id)


async def setup(bot: commands.Bot):
    await bot.add_cog(MemberOnboard(bot))


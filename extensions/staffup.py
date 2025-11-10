import asyncio
import traceback
from datetime import datetime, timezone
import aiohttp
import discord
from discord.ext import commands, tasks
from bot import logger
from discord import Embed
from config import STAFFUP_CHANNEL

from utils.vatsim import parse_vatsim_logon_time

online_zdc_controllers: list = []
staffup_channel = STAFFUP_CHANNEL

class Staffup(commands.Cog):
    """Staffup related commands."""

    def __init__(self, bot):
        self.bot = bot
        logger.info("Staffup extension initialized.")

        @commands.Cog.listener()
        async def on_ready(self):
            logger.info("Staffup extension on_ready fired.")
            if not self.check_online_controllers.is_running():
                logger.info("Starting check_online_controllers task loop.")
                self.check_online_controllers.start()
            else:
                logger.info("check_online_controllers task loop is already running.")

            def cog_unload(self):
                logger.info("Staffup cog unloaded. Stopping check_online_controllers task loop.")
                self.check_online_controllers.cancel()


    @tasks.loop(seconds=15.0)
    async def check_online_controllers(self):
        """Check for online controllers and update Staffup accordingly."""
        logger.info("Checking online controllers for Staffup update...")
        global online_zdc_controllers

        try:
            await self.bot.wait_until_ready()

            staffup_channel_id = self.bot.config.get("STAFFUP_CHANNEL")
            if not staffup_channel_id:
                logger.warning("STAFFUP_CHANNEL not configured. Skipping Staffup update.")
                return

            async with aiohttp.ClientSession() as session:
                async with session.get("https://live.env.vnas.vatsim.net/data-feed/controllers.json", timeout=10) as response:
                    if response.status == 200:
                        data = await response.json()
                        all_controllers = data["controllers"]
                        current_vatsim_controllers = []

                        for controller in all_controllers:
                            if controller["artccId"] == "ZDC":
                                current_vatsim_controllers.append(controller)

                        current_online_cids = {ctrl['cid'] for ctrl in online_zdc_controllers}
                        vatsim_online_cids = {ctrl['cid'] for ctrl in current_vatsim_controllers}

                        went_offline_cids = current_online_cids - vatsim_online_cids

                        for cid in went_offline_cids:
                            offline_ctrl_data = next((c for c in online_zdc_controllers if c['cid'] == cid), None)

                            if offline_ctrl_data and offline_ctrl_data['frequency'] != "199.998":
                                now_utc = datetime.now(timezone.utc)
                                login_time = offline_ctrl_data.get('login_time_utc')

                                duration_str = "N/A"
                                login_time_dt = None

                                if login_time:
                                    if isinstance(login_time, str):
                                        try:
                                            login_time_dt = parse_vatsim_logon_time(login_time)
                                        except Exception as parse_e:
                                            print(
                                                f"Error parsing stored login_time string for {offline_ctrl_data['callsign']}: {parse_e}")
                                            login_time_dt = None
                                    elif isinstance(login_time, datetime):

                                        if login_time.tzinfo is None:
                                            login_time_dt = login_time.replace(tzinfo=timezone.utc)
                                        else:
                                            login_time_dt = login_time
                                    else:
                                        print(
                                            f"Unexpected type for login_time_utc for {offline_ctrl_data['callsign']}: {type(login_time)}")
                                if login_time_dt:
                                    try:
                                        duration = now_utc - login_time_dt
                                        days = duration.days
                                        hours, remainder = divmod(duration.seconds, 3600)
                                        minutes, seconds = divmod(remainder, 60)

                                        duration_parts = []
                                        if days > 0:
                                            duration_parts.append(f"{days}d")
                                        if hours > 0:
                                            duration_parts.append(f"{hours}h")
                                        if minutes > 0:
                                            duration_parts.append(f"{minutes}m")
                                        if seconds > 0 and not duration_parts and duration.total_seconds() < 60:
                                            duration_parts.append(f"{seconds}s")

                                        duration_str = " ".join(duration_parts) if duration_parts else "0s"

                                    except Exception as dt_e:
                                        print(f"Error calculating duration for {offline_ctrl_data['callsign']}: {dt_e}")
                                        duration_str = "Error"

                                embed = Embed(
                                    title=f"{offline_ctrl_data['callsign']} - Offline",
                                    color=discord.Color.red()
                                )
                                embed.add_field(name="Name", value=f"{offline_ctrl_data['vatsimData']['realName']} ({offline_ctrl_data['vatsimData']['userRating']})",
                                                inline=True)
                                embed.add_field(name="Frequency", value=offline_ctrl_data['vatsimData']['primary.Frequency'], inline=True)

                                if login_time_dt:
                                    embed.add_field(name="Logon Time", value=f"<t:{int(login_time_dt.timestamp())}:t>",
                                                    inline=True)
                                    embed.add_field(name="Logoff Time", value=f"<t:{int(now_utc.timestamp())}:t>",
                                                    inline=True)
                                    embed.add_field(name="Duration", value=duration_str, inline=True)
                                else:
                                    embed.add_field(name="Session Info", value="Time data unavailable", inline=False)

                                embed.set_footer(text="vZDC Controller Status")
                                await staffup_channel.send(embed=embed)
                                print(f"Sent offline message for: {offline_ctrl_data['callsign']}")

                                online_zdc_controllers = [c for c in online_zdc_controllers if c['cid'] != cid]
                        came_online_cids = vatsim_online_cids - current_online_cids

                        for cid in came_online_cids:
                            online_ctrl_data = next((c for c in current_vatsim_controllers if c['cid'] == cid), None)

                            if online_ctrl_data and online_ctrl_data['frequency'] != "199.998":
                                logon_time_str = online_ctrl_data.get('logon_time')
                                if logon_time_str:
                                    try:
                                        online_ctrl_data['login_time_utc'] = parse_vatsim_logon_time(logon_time_str)
                                    except Exception:
                                        print(
                                            f"Could not parse VATSIM logon_time '{logon_time_str}' for CID {cid}. Using current UTC.")
                                        online_ctrl_data['login_time_utc'] = datetime.now(
                                            timezone.utc)
                                else:
                                    online_ctrl_data['login_time_utc'] = datetime.now(
                                        timezone.utc)

                                embed = Embed(
                                    title=f"{online_ctrl_data['callsign']} - Online",
                                    color=discord.Color.green()
                                )

                                online_zdc_controllers.append(online_ctrl_data)

                    else:
                        print(f"Could not fetch VATSIM Data. HTTP Status: {response.status}")
        except aiohttp.ClientError as e:
            print(f"Aiohttp client error occurred during VATSIM data fetch: {e}")
        except asyncio.TimeoutError:
            print("VATSIM data fetch timed out.")
        except Exception as e:
            print(f"An unexpected error occurred in check_online_controllers: {e}")
            traceback.print_exc()


async def setup(bot):
    await bot.add_cog(Staffup(bot))









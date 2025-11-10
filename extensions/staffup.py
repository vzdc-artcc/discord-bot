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

class Staffup(commands.Cog):
    """Staffup related commands."""

    def __init__(self, bot):
        self.bot = bot
        # instance storage for online controllers
        self.online_zdc_controllers: list = []
        # configured channel id (from config.STAFFUP_CHANNEL)
        self.staffup_channel_id = STAFFUP_CHANNEL
        logger.info("Staffup extension initialized.")

    @commands.Cog.listener()
    async def on_ready(self):
        """Start the background loop when the bot is ready."""
        logger.info("Staffup extension on_ready fired.")
        if not self.check_online_controllers.is_running():
            logger.info("Starting check_online_controllers task loop.")
            self.check_online_controllers.start()
        else:
            logger.info("check_online_controllers task loop is already running.")

    def cog_unload(self):
        logger.info("Staffup cog unloaded. Stopping check_online_controllers task loop.")
        try:
            self.check_online_controllers.cancel()
        except Exception:
            pass

    @tasks.loop(seconds=15.0)
    async def check_online_controllers(self):
        """Check for online controllers and update Staffup accordingly."""
        logger.info("Checking online controllers for Staffup update...")
        # use instance list
        online_ref = self.online_zdc_controllers

        try:
            await self.bot.wait_until_ready()

            # Resolve configured channel ID to a channel object
            staffup_channel = None
            try:
                if self.staffup_channel_id:
                    staffup_channel = self.bot.get_channel(self.staffup_channel_id)
                    if staffup_channel is None:
                        # attempt API fetch as fallback
                        try:
                            staffup_channel = await self.bot.fetch_channel(self.staffup_channel_id)
                        except Exception:
                            staffup_channel = None
            except Exception:
                staffup_channel = None

            if staffup_channel is None:
                logger.warning("STAFFUP_CHANNEL not found or not configured. Skipping Staffup update.")
                return

            async with aiohttp.ClientSession() as session:
                async with session.get("https://live.env.vnas.vatsim.net/data-feed/controllers.json", timeout=10) as response:
                    if response.status == 200:
                        data = await response.json()
                        all_controllers = data["controllers"]
                        current_vatsim_controllers = []

                        for controller in all_controllers:
                            if controller["artccId"] == "ZDC" and not controller['isObserver']:
                                current_vatsim_controllers.append(controller)

                        current_online_cids = {ctrl['vatsimData']['cid'] for ctrl in online_ref}
                        vatsim_online_cids = {ctrl['vatsimData']['cid'] for ctrl in current_vatsim_controllers}

                        went_offline_cids = current_online_cids - vatsim_online_cids

                        for cid in went_offline_cids:
                            offline_ctrl_data = next((c for c in online_ref if c['vatsimData']['cid'] == cid), None)

                            if offline_ctrl_data and offline_ctrl_data['isActive']:
                                now_utc = datetime.now(timezone.utc)
                                login_time = offline_ctrl_data.get('login_time_utc')

                                duration_str = "N/A"
                                login_time_dt = None

                                if login_time:
                                    if isinstance(login_time, str):
                                        try:
                                            login_time_dt = parse_vatsim_logon_time(login_time)
                                        except Exception as parse_e:
                                            logger.info(
                                                f"Error parsing stored login_time string for {offline_ctrl_data['vatsimData']['callsign']}: {parse_e}")
                                            login_time_dt = None
                                    elif isinstance(login_time, datetime):

                                        if login_time.tzinfo is None:
                                            login_time_dt = login_time.replace(tzinfo=timezone.utc)
                                        else:
                                            login_time_dt = login_time
                                    else:
                                        logger.info(
                                            f"Unexpected type for login_time_utc for {offline_ctrl_data['vatsimData']['callsign']}: {type(login_time)}")
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
                                        print(f"Error calculating duration for {offline_ctrl_data['vatsimData']['callsign']}: {dt_e}")
                                        duration_str = "Error"

                                embed = Embed(
                                    title=f"{offline_ctrl_data['vatsimData']['callsign']} - Offline",
                                    color=discord.Color.red()
                                )
                                embed.add_field(name="Name", value=f"{offline_ctrl_data['vatsimData']['realName']} ({offline_ctrl_data['vatsimData']['userRating']})",
                                                inline=True)
                                embed.add_field(name="Frequency", value=f"{offline_ctrl_data['vatsimData']['primaryFrequency']/1e6:.3f}", inline=True)

                                if login_time_dt:
                                    embed.add_field(name="Logon Time", value=f"<t:{int(login_time_dt.timestamp())}:t>",
                                                    inline=True)
                                    embed.add_field(name="Logoff Time", value=f"<t:{int(now_utc.timestamp())}:t>",
                                                    inline=True)
                                    embed.add_field(name="Duration", value=duration_str, inline=True)
                                else:
                                    embed.add_field(name="Session Info", value="Time data unavailable", inline=False)

                                embed.set_footer(text="vZDC Controller Status")
                                try:
                                    await staffup_channel.send(embed=embed)
                                    logger.info(f"Sent offline message for: {offline_ctrl_data['vatsimData']['callsign']}")
                                except Exception as e:
                                    logger.exception("Failed to send staffup offline embed: %s", e)

                                # remove from our instance list
                                online_ref = [c for c in online_ref if c['vatsimData']['cid'] != cid]
                                self.online_zdc_controllers = online_ref
                        came_online_cids = vatsim_online_cids - current_online_cids

                        for cid in came_online_cids:
                            online_ctrl_data = next((c for c in current_vatsim_controllers if c['vatsimData']['cid'] == cid), None)

                            if online_ctrl_data:
                                logon_time_str = None
                                for key in ("loginTime", "logon_time", "logonTime"):
                                    val = online_ctrl_data.get(key)
                                    if val:
                                        logon_time_str = val
                                        break

                                if logon_time_str:
                                    try:
                                        online_ctrl_data['login_time_utc'] = parse_vatsim_logon_time(logon_time_str)
                                    except Exception:
                                        logger.warning(
                                            f"Could not parse VATSIM login time '{logon_time_str}' for CID {cid}. Using current UTC.")
                                        online_ctrl_data['login_time_utc'] = datetime.now(timezone.utc)
                                else:
                                    online_ctrl_data['login_time_utc'] = datetime.now(timezone.utc)

                                embed = Embed(
                                    title=f"{online_ctrl_data['vatsimData']['callsign']} - Online",
                                    color=discord.Color.green()
                                )
                                embed.add_field(name="Name", value=f"{online_ctrl_data['vatsimData']['realName']} ({online_ctrl_data['vatsimData']['userRating']})")
                                embed.add_field(name="Frequency", value=f"{online_ctrl_data['vatsimData']['primaryFrequency']/1e6:.3f}", inline=True)
                                embed.add_field(name="Logon Time", value=f"<t:{int(online_ctrl_data['login_time_utc'].timestamp())}:t>", inline=True)

                                for pos in online_ctrl_data.get('positions', []):
                                    try:
                                        if pos.get('facilityId') == online_ctrl_data.get('primaryFacilityId'):
                                            continue

                                        # Only include positions that are active
                                        if not pos.get('isActive', False):
                                            continue

                                        # Format frequency (feed gives frequency as integer in Hz in many cases)
                                        freq = pos.get('frequency')
                                        if isinstance(freq, (int, float)):
                                            freq_str = f"{freq/1e6:.3f}"
                                        else:
                                            freq_str = str(freq) if freq is not None else "N/A"

                                        label = pos.get('positionName') or pos.get('defaultCallsign') or pos.get('radioName') or pos.get('positionId')

                                        embed.add_field(name="Additional Position", value=f"{pos.get('facilityName')} - {label} ({freq_str})", inline=True)
                                    except Exception as e:
                                        logger.exception("Error processing additional position for %s: %s", online_ctrl_data['vatsimData'].get('callsign'), e)

                                embed.set_footer(text="vZDC Controller Status")

                                await staffup_channel.send(embed=embed)
                                logger.info(f"Sent online message for: {online_ctrl_data['vatsimData']['callsign']}")

                                online_ref.append(online_ctrl_data)
                                self.online_zdc_controllers = online_ref

                    else:
                        logger.info(f"Could not fetch VATSIM Data. HTTP Status: {response.status}")
        except aiohttp.ClientError as e:
            logger.info(f"Aiohttp client error occurred during VATSIM data fetch: {e}")
        except asyncio.TimeoutError:
            logger.info("VATSIM data fetch timed out.")
        except Exception as e:
            logger.info(f"An unexpected error occurred in check_online_controllers: {e}")
            traceback.print_exc()


async def setup(bot):
    await bot.add_cog(Staffup(bot))

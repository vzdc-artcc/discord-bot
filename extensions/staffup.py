import asyncio
import discord
import aiohttp
import traceback
from datetime import datetime

from discord import Embed
from discord.ext import commands, tasks

from config import watched_positions, atc_rating, STAFFUP_CHANNEL, VATUSA_API_KEY

online_zdc_controllers = []

def parse_vatsim_logon_time(logon_time_str: str) -> datetime:

    parts = logon_time_str.replace('Z', '').split('.')
    main_part = parts[0]

    if len(parts) > 1:
        fractional_seconds = parts[1]
        truncated_fractional = fractional_seconds[:6]
        logon_time_str_corrected = f"{main_part}.{truncated_fractional}+00:00"
    else:
        logon_time_str_corrected = f"{main_part}+00:00"

    return datetime.fromisoformat(logon_time_str_corrected)


class Staffup(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        print("Staffup Cog initialized.")

    @commands.Cog.listener()
    async def on_ready(self):
        print("Staffup cog's on_ready fired.")
        if not self.check_online_controllers.is_running():
            print("Starting check_online_controllers task loop.")
            self.check_online_controllers.start()
        else:
            print("check_online_controllers task loop is already running.")

    def cog_unload(self):
        print("Staffup cog unloaded. Stopping check_online_controllers task loop.")
        self.check_online_controllers.cancel()

    async def get_real_name(self, cid: str) -> str:
        if not VATUSA_API_KEY:
            print("Warning: VATUSA_API_KEY not set. Cannot fetch real names.")
            return cid

        url = f"https://api.vatusa.net/v2/user/{cid}"
        headers = {"Authorization": f"Token {VATUSA_API_KEY}"}

        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(url, headers=headers, timeout=5) as response:
                    if response.status == 200:
                        user_data = await response.json()
                        data_payload = user_data.get('data', {})
                        first_name = data_payload.get('fname')
                        last_name = data_payload.get('lname')
                        if first_name and last_name:
                            return f"{first_name} {last_name}"
                        else:
                            print(f"VATUSA API: Name not found in data for CID {cid}. Response: {user_data}")
                            return cid
                    else:
                        print(f"VATUSA API Error for CID {cid}: HTTP {response.status} - {await response.text()}")
                        return cid
        except aiohttp.ClientError as e:
            print(f"VATUSA API Client Error for CID {cid}: {e}")
            return cid
        except asyncio.TimeoutError:
            print(f"VATUSA API Timeout for CID {cid}.")
            return cid
        except Exception as e:
            print(f"Unexpected error fetching real name for CID {cid}: {e}")
            traceback.print_exc()
            return cid

    @tasks.loop(seconds=15.0)
    async def check_online_controllers(self):
        global online_zdc_controllers

        try:
            await self.bot.wait_until_ready()

            staffup_channel = self.bot.get_channel(STAFFUP_CHANNEL)
            if not staffup_channel:
                print(f"Error: Staffup channel with ID {STAFFUP_CHANNEL} not found or inaccessible.")
                return

            async with aiohttp.ClientSession() as session:
                async with session.get("https://data.vatsim.net/v3/vatsim-data.json", timeout=10) as response:
                    if response.status == 200:
                        data = await response.json()
                        all_controllers = data["controllers"]

                        current_vatsim_controllers = []
                        for controller in all_controllers:
                            callsign = controller.get('callsign')
                            cid = controller.get('cid')
                            vatsim_name = controller.get('name', 'N/A')

                            if isinstance(callsign, str) and callsign.startswith(tuple(watched_positions)):

                                display_name = vatsim_name  # Default to VATSIM name
                                if cid and vatsim_name == str(cid):
                                    fetched_name = await self.get_real_name(str(cid))
                                    if fetched_name != str(cid):
                                        display_name = fetched_name

                                current_vatsim_controllers.append(
                                    {
                                        'callsign': callsign,
                                        'name': vatsim_name,
                                        'display_name': display_name,
                                        'rating': controller.get('rating', -1),
                                        'frequency': controller.get('frequency', 'N/A'),
                                        'cid': cid,
                                        'logon_time': controller.get('logon_time')
                                    }
                                )

                        current_online_cids = {ctrl['cid'] for ctrl in online_zdc_controllers}
                        vatsim_online_cids = {ctrl['cid'] for ctrl in current_vatsim_controllers}

                        went_offline_cids = current_online_cids - vatsim_online_cids

                        for cid in went_offline_cids:
                            offline_ctrl_data = next((c for c in online_zdc_controllers if c['cid'] == cid), None)

                            if offline_ctrl_data and offline_ctrl_data['frequency'] != "199.998":
                                now_utc = datetime.utcnow()
                                login_time = offline_ctrl_data.get('login_time_utc')

                                duration_str = "N/A"
                                login_time_dt = None
                                if login_time:
                                    try:
                                        login_time_dt = parse_vatsim_logon_time(login_time)

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
                                embed.add_field(name="Name",
                                                value=f"{offline_ctrl_data['display_name']} ({atc_rating.get(offline_ctrl_data['rating'], 'Unknown Rating')})",
                                                inline=True)
                                embed.add_field(name="Frequency", value=offline_ctrl_data['frequency'], inline=True)

                                if login_time_dt:
                                    embed.add_field(name="Online From", value=f"<t:{int(login_time_dt.timestamp())}:t>",
                                                    inline=True)
                                    embed.add_field(name="Offline At", value=f"<t:{int(now_utc.timestamp())}:t>",
                                                    inline=True)
                                    embed.add_field(name="Duration", value=duration_str, inline=True)
                                else:
                                    embed.add_field(name="Session Info", value="Time data unavailable", inline=False)

                                embed.set_footer(text="vZDC Controller Status")
                                await staffup_channel.send(embed=embed)
                                print(f"Sent offline message for: {offline_ctrl_data['callsign']}")

                                online_zdc_controllers = [c for c in online_zdc_controllers if c['cid'] != cid]

                        # Find controllers who just came online
                        came_online_cids = vatsim_online_cids - current_online_cids

                        for cid in came_online_cids:
                            online_ctrl_data = next((c for c in current_vatsim_controllers if c['cid'] == cid), None)

                            if online_ctrl_data and online_ctrl_data['frequency'] != "199.998":
                                logon_time_str = online_ctrl_data.get('logon_time')
                                if logon_time_str:
                                    try:
                                        # Use the new helper function here
                                        online_ctrl_data['login_time_utc'] = parse_vatsim_logon_time(logon_time_str)
                                    except Exception:  # Catch any parsing error
                                        print(
                                            f"Could not parse VATSIM logon_time '{logon_time_str}' for CID {cid}. Using current UTC.")
                                        online_ctrl_data['login_time_utc'] = datetime.utcnow()
                                else:
                                    online_ctrl_data['login_time_utc'] = datetime.utcnow()

                                embed = Embed(
                                    title=f"{online_ctrl_data['callsign']} - Online",
                                    color=discord.Color.green()
                                )
                                embed.add_field(name="Name",
                                                value=f"{online_ctrl_data['display_name']} ({atc_rating.get(online_ctrl_data['rating'], 'Unknown Rating')})",
                                                inline=True)
                                embed.add_field(name="Frequency", value=online_ctrl_data['frequency'], inline=True)
                                embed.add_field(name="Online From",
                                                value=f"<t:{int(online_ctrl_data['login_time_utc'].timestamp())}:t>",
                                                inline=True)
                                embed.set_footer(text="vZDC Controller Status")
                                await staffup_channel.send(embed=embed)
                                print(f"Sent online message for: {online_ctrl_data['callsign']}")

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
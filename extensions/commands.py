from datetime import datetime, timezone
import discord
from discord.ext import commands
from discord import app_commands
import logging
import os
import inspect
import requests
from utils.awc_api_helpers import get_category_color, qnh_hpa_to_inhg


class Commands(commands.Cog):
    """A Cog that provides slash (application) commands.

    - /ping: simple latency check
    - /manage echo <text>: an example management subcommand that echoes text (permission-checked)

    This Cog is implemented using discord.app_commands so commands are registered as slash commands.
    """

    def __init__(self, bot: commands.Bot):
        self.bot = bot
        self.logger = logging.getLogger(__name__)
        self.logger.info("Commands cog initialized.")
        # Track whether we've attempted to sync application commands to avoid repeated work
        self._synced = False

    @app_commands.command(name="ping", description="Check bot latency")
    async def ping(self, interaction: discord.Interaction):
        """Respond with pong and a rough latency measurement."""
        # Acknowledge quickly then follow up with latency
        await interaction.response.send_message("Pong! Calculating latency...")
        try:
            latency_ms = round(self.bot.latency * 1000)
        except Exception:
            latency_ms = None

        if latency_ms is not None:
            await interaction.followup.send(f"Latency: {latency_ms}ms")
        else:
            await interaction.followup.send("Latency information unavailable.")

    # Example command group
    manage_group = app_commands.Group(name="manage", description="Management helpers")

    @manage_group.command(name="echo", description="Echo text back (requires Manage Server)")
    async def manage_echo(self, interaction: discord.Interaction, text: str):
        """Echo a provided string back to the channel if the user has Manage Guild permission."""
        await interaction.response.defer(ephemeral=True)

        # Permission check performed at runtime to avoid decorator-specific behavior
        if not interaction.user.guild_permissions.manage_guild:
            await interaction.followup.send("You do not have permission to run this command.", ephemeral=True)
            return

        await interaction.followup.send(f"Echo: {text}", ephemeral=True)

    weather_group = app_commands.Group(name="weather", description="Weather Tools")

    @weather_group.command(name="metar", description="Returns the latest METAR for a given airport ICAO code.")
    async def weather_metar(self, interaction: discord.Interaction, icao: str):
        base_url = "https://aviationweather.gov/"
        guild = interaction.guild
        await interaction.response.defer()

        try:
            resp = requests.get(f"{base_url}/api/data/metar?ids={icao}&bbox=&format=json&taf=false&hours=1&date=",
                                timeout=10)
            resp.raise_for_status()
            payload = resp.json()
        except Exception as e:
            await interaction.followup.send(f"Failed to fetch METAR: {e}")
            return

        # Normalize payload to a single metar dict
        metar = None
        if isinstance(payload, dict):
            if isinstance(payload.get("data"), list) and payload["data"]:
                metar = payload["data"][0]
            elif payload:  # fallback if API returned a single dict with fields directly
                metar = payload
        elif isinstance(payload, list) and payload:
            metar = payload[0]

        if not metar:
            await interaction.followup.send(f"No METAR found for {icao.upper()}.")
            return

        # Parse report time (support int/float timestamp or ISO string)
        now_utc = datetime.now(timezone.utc)
        report_time_raw = metar.get("reportTime")
        report_dt = None
        if isinstance(report_time_raw, (int, float)):
            try:
                report_dt = datetime.fromtimestamp(int(report_time_raw), timezone.utc)
            except Exception:
                report_dt = None
        elif isinstance(report_time_raw, str):
            try:
                report_dt = datetime.fromisoformat(report_time_raw.replace("Z", "+00:00"))
            except Exception:
                try:
                    report_dt = datetime.strptime(report_time_raw, "%Y-%m-%dT%H:%M:%S%z")
                except Exception:
                    report_dt = None

        # Convert altimeter safely
        altim_raw = metar.get("altim") or metar.get("alt")
        try:
            altimeter = qnh_hpa_to_inhg(float(altim_raw)) if altim_raw is not None else "N/A"
        except Exception:
            altimeter = "N/A"

        color = get_category_color(metar.get("fltCat") or metar.get("flightCategory") or metar.get("flight_cat"))

        # Build embed; use report_dt (or now) for embed.timestamp
        embed = discord.Embed(
            title=f"METAR for {icao.upper()}",
            color=color,
            timestamp=now_utc,
        )

        # Use Discord timestamp markup so each user sees local time
        if report_dt:
            report_time_value = f"<t:{int(report_dt.timestamp())}:F>"
        else:
            report_time_value = "Unknown"

        embed.add_field(name="Raw METAR", value=str(metar.get("rawOb", "N/A")), inline=False)
        embed.add_field(name="Report Time", value=report_time_value, inline=True)

        # --- Formatting helpers for units ---
        def _fmt_temperature(raw):
            if raw is None:
                return "N/A"
            # Accept numeric or numeric strings
            if isinstance(raw, (int, float)):
                return f"{int(round(raw))}°C"
            if isinstance(raw, str):
                # if already contains a degree symbol or unit, return as-is
                if "°" in raw or "C" in raw or "F" in raw:
                    return raw
                try:
                    return f"{int(round(float(raw)))}°C"
                except Exception:
                    return raw
            if isinstance(raw, dict):
                for k in ("value", "temp", "temperature"):
                    if k in raw and raw[k] is not None:
                        try:
                            return f"{int(round(float(raw[k])))}°C"
                        except Exception:
                            return str(raw[k])
            return str(raw)

        def _fmt_altimeter_value(raw_altimeter):
            # raw_altimeter could be a float already converted to inHg, or a raw string/value
            if raw_altimeter is None:
                return "N/A"
            # If already a number (float/int), assume it's in inHg
            if isinstance(raw_altimeter, (int, float)):
                return f"{raw_altimeter:.2f} inHg"
            # If it's a string that already contains 'in' or 'Hg', return as-is
            if isinstance(raw_altimeter, str):
                if "in" in raw_altimeter.lower() or "hg" in raw_altimeter.lower():
                    return raw_altimeter
                # Try to parse numeric string
                try:
                    val = float(raw_altimeter)
                    return f"{val:.2f} inHg"
                except Exception:
                    return raw_altimeter
            # Fallback to str
            try:
                return f"{float(raw_altimeter):.2f} inHg"
            except Exception:
                return str(raw_altimeter)

        def _fmt_visibility(raw_vis):
            if raw_vis is None:
                return "N/A"
            # If it's a dict try common keys
            if isinstance(raw_vis, dict):
                for k in ("value", "visibility", "vis", "visib"):
                    if k in raw_vis and raw_vis[k] is not None:
                        raw_vis = raw_vis[k]
                        break
            # If it's numeric, decide meters vs statute miles by heuristic
            if isinstance(raw_vis, (int, float)):
                val = float(raw_vis)
                # Heuristic: values >= 1000 are likely meters
                if val >= 1000:
                    meters = val
                    sm = meters / 1609.344
                    # Show both SM and meters
                    return f"{sm:.1f} SM ({int(round(meters))} m)"
                else:
                    # treat as statute miles
                    return f"{val:.1f} SM"
            if isinstance(raw_vis, str):
                lower = raw_vis.lower()
                # if already annotated with units, return as-is
                if "sm" in lower or "m" in lower or "km" in lower or "nm" in lower:
                    return raw_vis
                # try numeric parse
                try:
                    val = float(raw_vis)
                    if val >= 1000:
                        meters = val
                        sm = meters / 1609.344
                        return f"{sm:.1f} SM ({int(round(meters))} m)"
                    else:
                        return f"{val:.1f} SM"
                except Exception:
                    return raw_vis
            return str(raw_vis)

        # Apply formatters
        altimeter_str = _fmt_altimeter_value(altimeter)
        temp_str = _fmt_temperature(metar.get("temp", None))
        dewp_str = _fmt_temperature(metar.get("dewp", None))

        embed.add_field(name="Altimeter", value=altimeter_str, inline=True)
        embed.add_field(name="Temperature", value=temp_str, inline=True)
        embed.add_field(name="Dewpoint", value=dewp_str, inline=True)

        # Combine wind direction and speed into one field (e.g. "110°, 10 kts")
        wdir_raw = metar.get("wdir") or metar.get("wind_dir") or metar.get("wdir_deg")
        wspd_raw = metar.get("wspd") or metar.get("wind_speed") or metar.get("wspd_kts")
        gust_raw = metar.get("gust") or metar.get("gust_kts") or metar.get("wgust") or metar.get("gust_speed")

        def _fmt_wind_dir(raw):
            if raw is None:
                return None
            if isinstance(raw, (int, float)):
                return f"{int(round(raw))}°"
            if isinstance(raw, str):
                # Preserve non-numeric values like 'VRB'
                try:
                    return f"{int(round(float(raw)))}°"
                except Exception:
                    return raw
            if isinstance(raw, dict):
                for k in ("value", "dir", "degree", "degrees", "deg"):
                    if k in raw and raw[k] is not None:
                        try:
                            return f"{int(round(float(raw[k])))}°"
                        except Exception:
                            return str(raw[k])
            return str(raw)

        def _parse_numeric(raw):
            """Try to extract a numeric value (float) from various raw forms or return None."""
            if raw is None:
                return None
            if isinstance(raw, (int, float)):
                return float(raw)
            if isinstance(raw, str):
                try:
                    return float(raw)
                except Exception:
                    return None
            if isinstance(raw, dict):
                for k in ("value", "speed", "spd", "kt", "kts", "speed_kts"):
                    if k in raw and raw[k] is not None:
                        try:
                            return float(raw[k])
                        except Exception:
                            continue
            return None

        dir_val_raw = _fmt_wind_dir(wdir_raw)
        spd_num = _parse_numeric(wspd_raw)
        gust_num = _parse_numeric(gust_raw)

        # Format output strings
        if spd_num == 0:
            # If true calm conditions; include gust if present
            if gust_num and gust_num > 0:
                wind_value = f"Calm, gust {int(round(gust_num))} kts"
            else:
                wind_value = "Calm"
        else:
            dir_val = dir_val_raw or "N/A"
            if spd_num is not None:
                spd_str = f"{int(round(spd_num))} kts"
            else:
                spd_str = "N/A"

            gust_str = ""
            if gust_num is not None and (spd_num is None or gust_num > (spd_num or 0)):
                gust_str = f", gust {int(round(gust_num))} kts"

            wind_value = f"{dir_val}, {spd_str}{gust_str}"

        embed.add_field(name="Wind", value=wind_value, inline=True)
        # Format visibility with units
        vis_str = _fmt_visibility(metar.get("visib", metar.get("visibility", None)))
        embed.add_field(name="Visibility", value=vis_str, inline=True)
        embed.add_field(name="Cloud Cover", value=str(metar.get("cover", "N/A")), inline=True)

        if metar.get("wxString"):
            embed.add_field(name="Precip", value=metar.get("wxString"), inline=False)

        cloud_text = ""
        if metar.get("clouds"):
            for cloud in metar["clouds"]:
                cloud_text += f"Cover: {cloud.get('cover', 'N/A')}, Bases: {cloud.get('base', 'N/A')}\n"
        embed.add_field(name="Clouds", value=cloud_text or "None", inline=False)

        embed.set_footer(text="vZDC", icon_url=guild.icon.url if guild and guild.icon else None)

        await interaction.followup.send(embed=embed)
    @commands.Cog.listener()
    async def on_ready(self):
        # Log when the bot is ready and the cog is active
        if not self.bot.is_ready():
            return

        # Only sync once per bot instance
        if self._synced or getattr(self.bot, "_app_commands_synced", False):
            self.logger.debug("Application commands already synced; skipping sync step.")
            return

        # Allow an optional developer guild to be specified via env var for fast sync during development
        guild_env = os.getenv("APP_COMMANDS_GUILD_ID")
        try:
            if guild_env:
                try:
                    guild_id = int(guild_env)
                    guild_obj = discord.Object(id=guild_id)
                    synced = await self.bot.tree.sync(guild=guild_obj)
                    self.logger.info("Synced %d application commands to guild %s.", len(synced), guild_id)
                except ValueError:
                    self.logger.warning("APP_COMMANDS_GUILD_ID is not a valid integer: %s. Falling back to global sync.", guild_env)
                    synced = await self.bot.tree.sync()
                    self.logger.info("Globally synced %d application commands.", len(synced))
            else:
                synced = await self.bot.tree.sync()
                self.logger.info("Globally synced %d application commands.", len(synced))

            # Mark synced to avoid repeating
            self._synced = True
            setattr(self.bot, "_app_commands_synced", True)
        except Exception as e:
            self.logger.exception("Failed to sync application commands: %s", e)

        self.logger.info("Commands cog ready. Application commands should be registered with Discord.")

    async def setup_hook(self):
        """Cog-specific setup hook to register app_commands with the bot's tree."""
        # Inspect the Cog class for app_commands.Command or app_commands.Group attributes and add them to the tree.
        # This ensures commands defined with @app_commands.command inside the Cog are actually present in bot.tree
        for name, member in inspect.getmembers(self.__class__):
            try:
                if isinstance(member, app_commands.Command) or isinstance(member, app_commands.Group):
                    try:
                        self.bot.tree.add_command(member)
                        logging.getLogger(__name__).debug("Added app command '%s' to bot.tree", name)
                    except Exception:
                        # Ignore if already registered or other tree-related errors
                        logging.getLogger(__name__).debug("Failed to add app command '%s' to bot.tree (it may already exist).", name)
            except Exception:
                # Be resilient to any unexpected inspect/getmembers issues
                continue


async def setup(bot: commands.Bot):
    """Async setup function used by discord.py to load this cog."""
    cog = Commands(bot)
    # Register the cog with the bot
    await bot.add_cog(cog)
    # Call the setup_hook to register app_commands
    await cog.setup_hook()

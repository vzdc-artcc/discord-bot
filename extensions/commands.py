from datetime import datetime, timezone
import discord
from discord.ext import commands
from discord import app_commands
import logging
import os
import inspect
import requests
import re
from utils.awc_api_helpers import get_category_color, qnh_hpa_to_inhg


def _find_first_key(obj, keys):
    """Recursively search dict/list 'obj' for the first occurrence of any key in 'keys' and return its value."""
    if obj is None:
        return None
    if isinstance(obj, dict):
        for k in keys:
            if k in obj and obj[k] is not None:
                return obj[k]
        for v in obj.values():
            res = _find_first_key(v, keys)
            if res is not None:
                return res
        return None
    if isinstance(obj, list):
        for item in obj:
            res = _find_first_key(item, keys)
            if res is not None:
                return res
        return None
    return None


class Commands(commands.Cog):
    """A Cog that provides slash (application) commands for weather (METAR/TAF)."""

    def __init__(self, bot: commands.Bot):
        self.bot = bot
        self.logger = logging.getLogger(__name__)
        self._synced = False

    weather_group = app_commands.Group(name="weather", description="Weather Tools")

    # -------------------- METAR --------------------
    @weather_group.command(name="metar", description="Returns the latest METAR for an ICAO.")
    async def weather_metar(self, interaction: discord.Interaction, icao: str):
        base_url = "https://aviationweather.gov/"
        guild = interaction.guild
        await interaction.response.defer()

        try:
            resp = requests.get(
                f"{base_url}/api/data/metar?ids={icao}&bbox=&format=json&taf=false&hours=1&date=",
                timeout=10,
            )
            resp.raise_for_status()
            payload = resp.json()
        except Exception as e:
            await interaction.followup.send(f"Failed to fetch METAR: {e}")
            return

        # Normalize payload to a single station dict
        metar = None
        if isinstance(payload, dict):
            if payload.get("features") and isinstance(payload.get("features"), list) and payload["features"]:
                first = payload["features"][0]
                metar = first.get("properties") if isinstance(first, dict) and first.get("properties") else first
            elif isinstance(payload.get("data"), list) and payload["data"]:
                metar = payload["data"][0]
            elif isinstance(payload.get("observations"), list) and payload["observations"]:
                metar = payload["observations"][0]
            elif payload:
                metar = payload
        elif isinstance(payload, list) and payload:
            metar = payload[0]

        if not metar:
            await interaction.followup.send(f"No METAR found for {icao.upper()}.")
            return

        # --- formatters ---
        def _fmt_temperature(raw):
            if raw is None:
                return "N/A"
            if isinstance(raw, (int, float)):
                return f"{int(round(raw))}°C"
            if isinstance(raw, str):
                if any(u in raw for u in ("°", "C", "F")):
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

        def _fmt_altimeter_value(raw_alt):
            if raw_alt is None:
                return "N/A"
            # if numeric, assume in inHg already
            if isinstance(raw_alt, (int, float)):
                return f"{raw_alt:.2f} inHg"
            if isinstance(raw_alt, str):
                if "in" in raw_alt.lower() or "hg" in raw_alt.lower():
                    return raw_alt
                try:
                    val = float(raw_alt)
                    return f"{val:.2f} inHg"
                except Exception:
                    return raw_alt
            # try to convert
            try:
                return f"{float(raw_alt):.2f} inHg"
            except Exception:
                return str(raw_alt)

        def _fmt_visibility(raw_vis):
            if raw_vis is None:
                return "N/A"
            if isinstance(raw_vis, dict):
                for k in ("value", "visibility", "vis", "visib"):
                    if k in raw_vis and raw_vis[k] is not None:
                        raw_vis = raw_vis[k]
                        break
            if isinstance(raw_vis, (int, float)):
                val = float(raw_vis)
                if val >= 1000:
                    meters = val
                    sm = meters / 1609.344
                    return f"{sm:.1f} SM ({int(round(meters))} m)"
                else:
                    return f"{val:.1f} SM"
            if isinstance(raw_vis, str):
                s = raw_vis.strip()
                # keep things like '10+' or '6'
                return s
            return str(raw_vis)

        # --- extract fields ---
        # time
        report_raw = metar.get("reportTime") or metar.get("obsTime") or metar.get("observation_time") or metar.get("receiptTime")
        report_dt = None
        if isinstance(report_raw, (int, float)):
            try:
                report_dt = datetime.fromtimestamp(int(report_raw), timezone.utc)
            except Exception:
                report_dt = None
        elif isinstance(report_raw, str):
            try:
                report_dt = datetime.fromisoformat(report_raw.replace("Z", "+00:00"))
            except Exception:
                report_dt = None

        # raw text
        raw_text = (
            metar.get("raw_text")
            or metar.get("rawText")
            or metar.get("rawOb")
            or metar.get("raw")
            or _find_first_key(metar, ["raw_text", "rawText", "raw", "rawOb", "raw_message"])
        )

        # altimeter candidates
        alt_keys = ("altim_in_hg", "altim", "alt", "sea_level_pressure_hpa", "sea_level_pressure_mb", "pressure_hpa", "pressure_mb", "qnh", "hpa", "slp")
        alt_raw = None
        alt_key_used = None
        for k in alt_keys:
            if k in metar and metar.get(k) is not None:
                alt_raw = metar.get(k)
                alt_key_used = k
                break

        # temperature / dewpoint
        temp_raw = metar.get("temp_c") or metar.get("temp") or metar.get("temperature") or _find_first_key(metar, ["temp", "temp_c"])
        dewp_raw = metar.get("dewpoint_c") or metar.get("dewp") or metar.get("dewpoint") or _find_first_key(metar, ["dewpoint", "dewp"])

        # wind fields (AWC uses wdir/wspd/wgst; other providers vary)
        wdir = metar.get("wind_dir_degrees") or metar.get("wdir") or metar.get("wdir_deg") or metar.get("wdir_raw")
        wspd = metar.get("wind_speed_kt") or metar.get("wspd") or metar.get("wspd_kts") or metar.get("wind_speed")
        wgust = metar.get("wind_gust_kt") or metar.get("wgst") or metar.get("wind_gust") or metar.get("gust")

        def _fmt_wind_dir(d):
            if d is None:
                return None
            try:
                return f"{int(round(float(d)))}°"
            except Exception:
                return str(d)

        def _num(v):
            if v is None:
                return None
            try:
                return float(v)
            except Exception:
                return None

        wspd_n = _num(wspd)
        wgust_n = _num(wgust)
        wdir_s = _fmt_wind_dir(wdir)

        if wspd_n == 0 or wspd_n is None and wdir in (0, "0"):
            # If speed is zero (or wdir 0 and wspd None), show Calm (unless gusts)
            if wgust_n and wgust_n > 0:
                wind_value = f"Calm, Gusts: {int(round(wgust_n))} Kts"
            else:
                wind_value = "Calm"
        else:
            spd_part = f"{int(round(wspd_n))} Kts" if wspd_n is not None else "N/A"
            gust_part = f", gust {int(round(wgust_n))} kts" if wgust_n is not None and (wspd_n is None or wgust_n > (wspd_n or 0)) else ""
            wind_value = f"{wdir_s or 'N/A'}, {spd_part}{gust_part}"

        # altimeter conversion: if key suggests hPa (sea_level_pressure_mb/hpa/slp) convert using helper
        alt_for_fmt = None
        if alt_raw is None:
            alt_for_fmt = None
        else:
            if alt_key_used in ("altim_in_hg",) or (isinstance(alt_raw, str) and ("in" in str(alt_raw).lower() or "hg" in str(alt_raw).lower())):
                alt_for_fmt = alt_raw
            else:
                # try to convert hPa to inHg
                try:
                    alt_for_fmt = qnh_hpa_to_inhg(alt_raw)
                except Exception:
                    alt_for_fmt = alt_raw

        altimeter_str = _fmt_altimeter_value(alt_for_fmt)
        temp_str = _fmt_temperature(temp_raw)
        dewp_str = _fmt_temperature(dewp_raw)
        vis_str = _fmt_visibility(metar.get("visibility_statute_mi") or metar.get("visib") or metar.get("visibility") or _find_first_key(metar, ["visib", "visibility"]))

        # flight category / color
        flt_cat = metar.get("flight_category") or metar.get("fltCat") or metar.get("flightCategory") or metar.get("flight_cat") or metar.get("flt_category")
        color = get_category_color(flt_cat)

        # Build embed
        embed = discord.Embed(title=f"METAR for {icao.upper()}", color=color, timestamp=datetime.now(timezone.utc))
        embed.add_field(name="Raw METAR", value=str(raw_text or "N/A"), inline=False)
        if report_dt:
            embed.add_field(name="Report Time", value=f"<t:{int(report_dt.timestamp())}:F>", inline=True)
        else:
            embed.add_field(name="Report Time", value="Unknown", inline=True)
        embed.add_field(name="Altimeter", value=altimeter_str, inline=True)
        embed.add_field(name="Temperature", value=temp_str, inline=True)
        embed.add_field(name="Dewpoint", value=dewp_str, inline=True)
        embed.add_field(name="Wind", value=wind_value, inline=True)
        embed.add_field(name="Visibility", value=vis_str, inline=True)

        # Clouds
        cloud_text = ""
        if metar.get("clouds"):
            for c in metar["clouds"]:
                cloud_text += f"Cover: {c.get('cover','N/A')}, Bases: {c.get('base','N/A')}\n"
        elif metar.get("cover"):
            cloud_text = str(metar.get("cover"))
        embed.add_field(name="Clouds", value=cloud_text or "None", inline=False)

        embed.set_footer(text="vZDC", icon_url=guild.icon.url if guild and guild.icon else None)
        await interaction.followup.send(embed=embed)

    # -------------------- TAF --------------------
    @weather_group.command(name="taf", description="Returns the latest TAF for an ICAO.")
    async def weather_taf(self, interaction: discord.Interaction, icao: str):
        base_url = "https://aviationweather.gov/"
        guild = interaction.guild
        await interaction.response.defer()

        try:
            resp = requests.get(f"{base_url}/api/data/taf?ids={icao}&format=json&hours=24&date=", timeout=10)
            resp.raise_for_status()
            payload = resp.json()
        except Exception as e:
            await interaction.followup.send(f"Failed to fetch TAF: {e}")
            return

        # normalize
        taf = None
        if isinstance(payload, dict):
            if payload.get("features") and isinstance(payload.get("features"), list) and payload["features"]:
                first = payload["features"][0]
                taf = first.get("properties") if isinstance(first, dict) and first.get("properties") else first
            elif isinstance(payload.get("data"), list) and payload["data"]:
                taf = payload["data"][0]
            elif isinstance(payload.get("observations"), list) and payload["observations"]:
                taf = payload["observations"][0]
            elif payload:
                taf = payload
        elif isinstance(payload, list) and payload:
            taf = payload[0]

        if not taf:
            await interaction.followup.send(f"No TAF found for {icao.upper()}.")
            return

        raw_taf = taf.get("rawTAF") or taf.get("rawText") or taf.get("raw_text") or _find_first_key(taf, ["rawTAF", "rawText", "raw_text", "raw"])

        embed = discord.Embed(title=f"TAF for {icao.upper()}", description=str(raw_taf or taf.get("rawText", "N/A")), color=discord.Color.blue())

        forecasts = taf.get("fcsts") or taf.get("forecast") or taf.get("periods") or taf.get("forecasts") or taf.get("fcst") or []

        def _to_dt(v):
            if v is None:
                return None
            if isinstance(v, (int, float)):
                try:
                    return datetime.fromtimestamp(int(v), timezone.utc)
                except Exception:
                    return None
            if isinstance(v, str):
                try:
                    return datetime.fromisoformat(v.replace("Z", "+00:00"))
                except Exception:
                    return None
            return None

        def _fmt_vis_taf(v):
            if v is None:
                return "N/A"
            if isinstance(v, (int, float)):
                return f"{v}sm"
            return str(v)

        def _parse_vis_sm(v):
            if v is None:
                return None
            try:
                if isinstance(v, (int, float)):
                    return float(v)
                s = str(v).strip()
                m = re.match(r"^(\d+(?:\.\d+)?)(?:\+)?", s)
                if m:
                    return float(m.group(1))
            except Exception:
                return None
            return None

        def _determine_cat(vis_raw, clouds):
            vis = _parse_vis_sm(vis_raw)
            lowest = None
            if isinstance(clouds, list):
                for c in clouds:
                    cover = (c.get("cover") or c.get("sky_cover") or "").upper()
                    base = c.get("base") or c.get("base_ft_agl") or c.get("cloud_base_ft_agl")
                    if cover in ("BKN", "OVC", "VV") and base:
                        try:
                            b = int(base)
                            if lowest is None or b < lowest:
                                lowest = b
                        except Exception:
                            continue
            if (vis is not None and vis < 1) or (lowest is not None and lowest < 500):
                return "LIFR"
            if (vis is not None and vis < 3) or (lowest is not None and lowest < 1000):
                return "IFR"
            if (vis is not None and vis < 5) or (lowest is not None and lowest < 3000):
                return "MVFR"
            if vis is not None or lowest is not None:
                return "VFR"
            return "N/A"

        if not forecasts and isinstance(raw_taf, str):
            embed.add_field(name="Raw TAF", value=raw_taf, inline=False)
            embed.set_footer(text="vZDC", icon_url=guild.icon.url if guild and guild.icon else None)
            await interaction.followup.send(embed=embed)
            return

        for fc in forecasts:
            time_from = _to_dt(fc.get("timeFrom") or fc.get("fcstTimeFrom") or fc.get("time_from"))
            time_to = _to_dt(fc.get("timeTo") or fc.get("fcstTimeTo") or fc.get("time_to"))
            if time_from:
                utc_heading = time_from.strftime("%d/%m %H%MZ")
                local_ts = f" (<t:{int(time_from.timestamp())}:F>)"
                heading = f"From {utc_heading}{local_ts}"
            else:
                heading = "From Unknown"

            # AWC fields wdir/wspd/wgst/visib
            wdir = fc.get("wdir") or fc.get("windDirDegrees") or fc.get("wind_dir")
            wspd = fc.get("wspd") or fc.get("windSpeedKt") or fc.get("wind_speed_kt")
            wgst = fc.get("wgst") or fc.get("wgst_kts") or fc.get("windGustKt") or fc.get("wind_gust_kt")

            try:
                wdir_s = (f"{int(round(float(wdir)))}°" if wdir is not None and str(wdir).strip() != "" else None)
            except Exception:
                wdir_s = (str(wdir) if wdir is not None else None)
            try:
                wspd_n = float(wspd) if wspd is not None and str(wspd).strip() != "" else None
            except Exception:
                wspd_n = None
            try:
                wgst_n = float(wgst) if wgst is not None and str(wgst).strip() != "" else None
            except Exception:
                wgst_n = None

            if wspd_n == 0:
                wind_val = "Calm" if not wgst_n else f"Calm, Gusts: {int(round(wgst_n))} Kts"
            else:
                spd_part = f"{int(round(wspd_n))} Kts" if wspd_n is not None else "N/A"
                gust_part = f", Gusts: {int(round(wgst_n))} Kts" if wgst_n is not None and (wspd_n is None or wgst_n > (wspd_n or 0)) else ""
                wind_val = f"{wdir_s or 'N/A'}, {spd_part}{gust_part}"

            vis_raw = fc.get("visib") or fc.get("vis") or fc.get("visibility") or fc.get("visibilityStatuteMiles")
            vis_str = _fmt_vis_taf(vis_raw)
            flt = fc.get("flightCategory") or _determine_cat(vis_raw, fc.get("clouds") or [])
            flt = flt or "N/A"

            lines = []
            lines.append(f"Wind:{wind_val}")
            lines.append(f"Flight Rules: {flt}")
            lines.append(f"Visibility: {vis_str}")
            if fc.get("wxString") or fc.get("weather"):
                lines.append(f"Weather: {fc.get('wxString') or fc.get('weather')}")

            embed.add_field(name=heading, value="\n".join(lines), inline=False)

        embed.set_footer(text="vZDC", icon_url=guild.icon.url if guild and guild.icon else None)
        await interaction.followup.send(embed=embed)

    # -------------------- registration helpers --------------------
    async def setup_hook(self):
        """Cog-specific setup hook to register app_commands with the bot's tree."""
        try:
            self.bot.tree.add_command(self.weather_group)
            logging.getLogger(__name__).debug("Added weather_group to bot.tree")
        except Exception:
            logging.getLogger(__name__).debug("weather_group already present in bot.tree or failed to add")

        for name, member in inspect.getmembers(self.__class__):
            try:
                if isinstance(member, app_commands.Command) or isinstance(member, app_commands.Group):
                    try:
                        if member is self.weather_group:
                            continue
                        self.bot.tree.add_command(member)
                        logging.getLogger(__name__).debug("Added app command '%s' to bot.tree", name)
                    except Exception:
                        logging.getLogger(__name__).debug("Failed to add app command '%s' to bot.tree (it may already exist).", name)
            except Exception:
                continue

    @commands.Cog.listener()
    async def on_ready(self):
        # avoid repeated syncs
        if self._synced or getattr(self.bot, "_app_commands_synced", False):
            return

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

            self._synced = True
            setattr(self.bot, "_app_commands_synced", True)
        except Exception as e:
            self.logger.exception("Failed to sync application commands: %s", e)


async def setup(bot: commands.Bot):
    """Async setup function used by discord.py to load this cog."""
    cog = Commands(bot)
    await bot.add_cog(cog)
    try:
        await cog.setup_hook()
    except Exception:
        pass

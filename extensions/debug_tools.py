import json
import logging
import io
import discord
from discord.ext import commands
import config as cfg

logger = logging.getLogger(__name__)

class DebugTools(commands.Cog):
    def __init__(self, bot: commands.Bot):
        self.bot = bot

    @commands.command(name="listchannels")
    async def list_channels(self, ctx: commands.Context):
        """List visible text channels in this guild with IDs and permissions for the bot."""
        guild = ctx.guild
        if guild is None:
            await ctx.send("This command must be run in a guild.")
            return

        lines = []
        me = guild.me
        for ch in guild.text_channels:
            try:
                perms = ch.permissions_for(me)
                can_read = perms.read_messages if perms is not None else False
            except Exception:
                can_read = False
            lines.append(f"{ch.name} — id={ch.id} — can_read={can_read}")

        if not lines:
            await ctx.send("No text channels visible or bot cannot access channel list.")
            return

        # send in multiple messages if needed
        msg = "\n".join(lines)
        if len(msg) > 1900:
            # too long for one message, send as file
            await ctx.send(file=discord.File(fp=io.BytesIO(msg.encode()), filename="channels.txt"))
        else:
            await ctx.send(f"Text channels (name — id — bot_can_read):\n{msg}")

    @commands.command(name="checkbreakboard")
    async def check_breakboard(self, ctx: commands.Context):
        """Show the configured breakboard channel id and whether the bot can resolve it."""
        guild = ctx.guild
        if guild is None:
            await ctx.send("This command must be run inside a guild.")
            return

        guild_id = guild.id
        guild_cfg = cfg.get_guild_config(guild_id)
        cbid = guild_cfg.get_channel("break_board_channel_id")

        info = {
            "guild_id": guild_id,
            "guild_name": guild.name,
            "configured_break_board_channel_id": cbid,
        }

        # Try guild cache
        try:
            guild_channel = guild.get_channel(cbid) if cbid is not None else None
        except Exception:
            guild_channel = None

        # Try bot cache
        try:
            bot_channel = self.bot.get_channel(cbid) if cbid is not None else None
        except Exception:
            bot_channel = None

        info.update({
            "guild.get_channel_returns": str(type(guild_channel)) if guild_channel is not None else None,
            "bot.get_channel_returns": str(type(bot_channel)) if bot_channel is not None else None,
        })

        # Permission check if channel object present
        perm_info = None
        if guild_channel is not None:
            try:
                perms = guild_channel.permissions_for(guild.me)
                perm_info = {
                    "read_messages": bool(perms.read_messages),
                    "send_messages": bool(perms.send_messages),
                }
            except Exception:
                perm_info = {"error": "failed to evaluate permissions"}

        info["resolved_permissions_on_guild_channel"] = perm_info

        # Report which file the config module is using and whether it contains the guild entry
        try:
            import pathlib, hashlib
            cfg_path = pathlib.Path(cfg.GUILD_CONFIG_FILE)
            info["guild_config_file"] = str(cfg.GUILD_CONFIG_FILE)
            info["guild_config_file_exists"] = cfg_path.exists()
            if cfg_path.exists():
                raw_text = cfg_path.read_text()
                info["guild_config_file_size"] = cfg_path.stat().st_size
                info["guild_config_file_md5"] = hashlib.md5(raw_text.encode()).hexdigest()
                try:
                    raw_json = json.loads(raw_text)
                    info["raw_file_entry_for_guild"] = raw_json.get(str(guild_id))
                except Exception:
                    info["raw_file_entry_for_guild"] = "<unparseable>"
        except Exception as e:
            info["guild_config_file_error"] = str(e)

        # Send JSON blob so it's easy to copy/paste
        await ctx.send(f"```{json.dumps(info, indent=2)}```")

    @commands.command(name="dumpconfig")
    async def dump_config(self, ctx: commands.Context):
        """Print the guild config as read from the config module."""
        guild = ctx.guild
        if guild is None:
            await ctx.send("This command must be run in a guild.")
            return
        cfg_obj = cfg.get_guild_config(guild.id)
        try:
            cfg_json = json.dumps(cfg_obj.as_dict(), indent=2)
        except Exception:
            cfg_json = str(cfg_obj.as_dict())
        # send as file if large
        if len(cfg_json) > 1900:
            await ctx.send(file=discord.File(fp=io.BytesIO(cfg_json.encode()), filename="guild_config.json"))
        else:
            # Send as a single-line f-string code block to avoid unterminated string syntax
            await ctx.send(f"```{cfg_json}```")

    @commands.command(name="setbreakboard")
    @commands.has_guild_permissions(manage_guild=True)
    async def set_breakboard(self, ctx: commands.Context, channel: discord.TextChannel):
        """Set and persist the breakboard channel for this guild. Usage: `!setbreakboard #channel`"""
        guild = ctx.guild
        if guild is None:
            await ctx.send("This command must be run in a guild.")
            return

        # Load current config dict and update the break_board_channel_id
        current = cfg.get_guild_config(guild.id).as_dict()
        if "channels" not in current:
            current["channels"] = {}
        current["channels"]["break_board_channel_id"] = int(channel.id)

        try:
            cfg.save_guild_config(guild.id, current)
            await ctx.send(f"Set breakboard channel to {channel.mention} (id={channel.id}) and saved to config.")
        except Exception as e:
            await ctx.send(f"Failed to save breakboard channel: {e}")
            logger.exception("Failed to persist breakboard channel configuration")


async def setup(bot):
    await bot.add_cog(DebugTools(bot))

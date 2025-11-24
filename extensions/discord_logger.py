import discord
from discord.ext import commands
import config
import logging
from datetime import datetime, timezone
from typing import Optional, List, Any

logger = logging.getLogger("discord_logger")

# Helper truncation constants
MAX_FIELD_LENGTH = 1900


def _truncate(text: Optional[str], max_len: int = MAX_FIELD_LENGTH) -> str:
    if text is None:
        return ""
    s = str(text)
    if len(s) <= max_len:
        return s
    return s[: max_len - 12] + "...[truncated]"


def _format_footer(author_id: Optional[int], message_id: Optional[int], ts: Optional[datetime]) -> str:
    """Format footer like: "Author: <id> | Message ID: <id>•<discord timestamp>".

    Use Discord's time formatting (<t:unix:f>) so clients show "Today at ..." or "Yesterday at ..." when appropriate.
    """
    parts = []
    if author_id is not None:
        parts.append(f"Author: {author_id}")
    if message_id is not None:
        parts.append(f"Message ID: {message_id}")

    time_str = None
    if ts is not None:
        try:
            # Use Discord's timestamp format so the client will render relative "Today/Yesterday" labels
            unix_ts = int(ts.timestamp())
            time_str = f"<t:{unix_ts}:f>"
        except Exception:
            try:
                time_str = str(ts)
            except Exception:
                time_str = None

    footer = " | ".join(parts) if parts else ""
    if time_str:
        # Compact bullet separator (no spaces) as requested: "Message ID: 123•<t:...:f>"
        footer = f"{footer}•{time_str}" if footer else time_str
    return footer


class DiscordLogger(commands.Cog):
    """Per-guild detailed logging cog.

    Reads `logging_channel_id` from `config.get_channel_for_guild(guild_id, 'logging_channel_id')`.
    Sends embeds when possible, falls back to plaintext.
    """

    def __init__(self, bot: commands.Bot):
        self.bot = bot
        # Simple in-memory marker for guilds where sending failed recently to avoid spam
        self._send_failure_cooldown = {}

    # --- Helpers ---
    def _get_log_channel_id(self, guild_id: int) -> Optional[int]:
        return config.get_channel_for_guild(guild_id, "logging_channel_id")

    async def _resolve_channel(self, guild: discord.Guild):
        cid = self._get_log_channel_id(guild.id)
        if not cid:
            return None
        ch = guild.get_channel(int(cid))
        if ch is None:
            try:
                ch = await self.bot.fetch_channel(int(cid))
            except Exception:
                return None
        return ch

    async def _safe_send(self, channel: discord.abc.Messageable, *, embed: Optional[discord.Embed] = None, content: Optional[str] = None):
        try:
            # Try embed first
            if embed is not None:
                try:
                    await channel.send(embed=embed)
                    return
                except discord.Forbidden:
                    # Fall back to text
                    pass
                except Exception:
                    # If embed send fails for other reasons, attempt text fallback
                    logger.exception("Failed to send embed to log channel; falling back to text")
            if content is None:
                content = "(no content)"
            await channel.send(content)
        except discord.Forbidden:
            logger.warning("Missing permission to send logs to channel %s", getattr(channel, "id", None))
        except Exception:
            logger.exception("Unexpected error sending log message")

    async def _fetch_audit_actor(self, guild: discord.Guild, action: Any, *, target_id: Optional[int] = None, lookback_seconds: int = 10):
        """Best-effort: find who performed an action via audit logs.

        Returns tuple (member or None, reason or None).
        """
        try:
            if not guild.me.guild_permissions.view_audit_log:
                return None, None
        except Exception:
            return None, None

        try:
            async for entry in guild.audit_logs(action=action, limit=6):
                # If we have a target_id, try to match it
                if target_id is not None and getattr(entry.target, "id", None) != target_id:
                    continue
                # Prefer very recent entries
                if (datetime.now(timezone.utc) - entry.created_at).total_seconds() > lookback_seconds:
                    continue
                return entry.user, getattr(entry, "reason", None)
        except Exception:
            logger.exception("Failed to query audit logs for guild %s", getattr(guild, "id", None))
        return None, None

    def _build_basic_embed(self, title: str, color: discord.Color = discord.Color.light_grey(), description: Optional[str] = None):
        e = discord.Embed(title=title, color=color)
        if description:
            e.description = _truncate(description)
        e.timestamp = datetime.now(timezone.utc)
        return e

    # --- Events ---
    @commands.Cog.listener()
    async def on_member_join(self, member: discord.Member):
        guild = member.guild
        if guild is None:
            return
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        desc = f"Member: {member} (ID {member.id}) joined. Account created: {member.created_at.isoformat()}"
        embed = self._build_basic_embed("🟢 Member Join", discord.Color.green(), desc)
        embed.set_thumbnail(url=getattr(member.display_avatar, "url", None))
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_member_remove(self, member: discord.Member):
        guild = member.guild
        if guild is None:
            return
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        # Try to determine if this was a kick via audit logs
        actor, reason = await self._fetch_audit_actor(guild, discord.AuditLogAction.kick, target_id=member.id)
        title = "🔴 Member Removed"
        desc_lines = [f"Member: {member} (ID {member.id})"]
        if actor:
            desc_lines.append(f"Removed by: {actor} (ID {actor.id})")
        else:
            desc_lines.append("Removed by: (unknown)")
        if reason:
            desc_lines.append(f"Reason: {reason}")
        desc = "\n".join(desc_lines)
        embed = self._build_basic_embed(title, discord.Color.red(), desc)
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_member_ban(self, guild: discord.Guild, user: discord.User):
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        actor, reason = await self._fetch_audit_actor(guild, discord.AuditLogAction.ban, target_id=user.id)
        title = "🔨 Member Banned"
        desc_lines = [f"User: {user} (ID {user.id})"]
        if actor:
            desc_lines.append(f"Banned by: {actor} (ID {actor.id})")
        else:
            desc_lines.append("Banned by: (unknown)")
        if reason:
            desc_lines.append(f"Reason: {reason}")
        embed = self._build_basic_embed(title, discord.Color.dark_red(), "\n".join(desc_lines))
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_member_unban(self, guild: discord.Guild, user: discord.User):
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        actor, reason = await self._fetch_audit_actor(guild, discord.AuditLogAction.unban, target_id=user.id)
        title = "🟡 Member Unbanned"
        desc_lines = [f"User: {user} (ID {user.id})"]
        if actor:
            desc_lines.append(f"Unbanned by: {actor} (ID {actor.id})")
        else:
            desc_lines.append("Unbanned by: (unknown)")
        if reason:
            desc_lines.append(f"Reason: {reason}")
        embed = self._build_basic_embed(title, discord.Color.gold(), "\n".join(desc_lines))
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_guild_role_create(self, role: discord.Role):
        guild = role.guild
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        actor, reason = await self._fetch_audit_actor(guild, discord.AuditLogAction.role_create, target_id=role.id)
        title = "🔧 Role Created"
        desc = f"Role: {role.name} (ID {role.id}) created"
        if actor:
            desc += f" by {actor} (ID {actor.id})"
        if reason:
            desc += f" — Reason: {reason}"
        embed = self._build_basic_embed(title, discord.Color.orange(), desc)
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_guild_role_delete(self, role: discord.Role):
        guild = role.guild
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        actor, reason = await self._fetch_audit_actor(guild, discord.AuditLogAction.role_delete, target_id=role.id)
        title = "🗑️ Role Deleted"
        desc = f"Role: {role.name} (ID {role.id}) deleted"
        if actor:
            desc += f" by {actor} (ID {actor.id})"
        if reason:
            desc += f" — Reason: {reason}"
        embed = self._build_basic_embed(title, discord.Color.dark_grey(), desc)
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_guild_role_update(self, before: discord.Role, after: discord.Role):
        guild = after.guild
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        actor, reason = await self._fetch_audit_actor(guild, discord.AuditLogAction.role_update, target_id=after.id)
        title = "✏️ Role Edited"
        diffs = []
        if before.name != after.name:
            diffs.append(f"Name: '{before.name}' -> '{after.name}'")
        if before.permissions != after.permissions:
            diffs.append("Permissions changed")
        if before.color != after.color:
            diffs.append(f"Color changed")
        if before.hoist != after.hoist:
            diffs.append(f"Hoist changed: {before.hoist} -> {after.hoist}")
        if not diffs:
            diffs.append("No visible changes")
        desc = f"Role: {after.name} (ID {after.id})\n" + "; ".join(diffs)
        if actor:
            desc += f"\nEdited by: {actor} (ID {actor.id})"
        if reason:
            desc += f"\nReason: {reason}"
        embed = self._build_basic_embed(title, discord.Color.blue(), desc)
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_member_update(self, before: discord.Member, after: discord.Member):
        # Detect role adds/removes
        guild = after.guild
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        before_roles = set(before.roles)
        after_roles = set(after.roles)
        added = after_roles - before_roles
        removed = before_roles - after_roles
        if not added and not removed:
            return
        # Try to attribute via audit logs (best-effort)
        actor, reason = await self._fetch_audit_actor(guild, discord.AuditLogAction.member_role_update, target_id=after.id)
        title = "🧾 Member Roles Updated"
        parts: List[str] = [f"Member: {after} (ID {after.id})"]
        if added:
            parts.append("Added roles: " + ", ".join(r.name for r in added))
        if removed:
            parts.append("Removed roles: " + ", ".join(r.name for r in removed))
        if actor:
            parts.append(f"By: {actor} (ID {actor.id})")
        else:
            parts.append("By: (unknown)")
        if reason:
            parts.append(f"Reason: {reason}")
        embed = self._build_basic_embed(title, discord.Color.purple(), "\n".join(parts))
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_message_delete(self, message: discord.Message):
        guild = message.guild
        if guild is None:
            return
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        # message may be partial (not cached)
        author = getattr(message, "author", None)
        title = "🗑️ Message Deleted"
        # Put identifying info in the description
        desc_parts = [f"Channel: {getattr(message.channel, 'name', repr(message.channel))} (ID {getattr(message.channel, 'id', 'unknown')})"]
        if author:
            desc_parts.append(f"Author: {author} (ID {author.id})")
        desc_parts.append(f"Message ID: {getattr(message, 'id', 'unknown')}")
        embed = self._build_basic_embed(title, discord.Color.dark_grey(), "\n".join(desc_parts))

        # Content field (use embed field so it displays cleanly)
        content = getattr(message, "content", None)
        content_value = _truncate(content) if content else "(unavailable)"
        try:
            embed.add_field(name="Content", value=content_value, inline=False)
        except Exception:
            # Fall back to appending to description
            embed.description = (embed.description or "") + "\nContent: " + content_value

        # Attachments as a separate field when present
        try:
            attachments = getattr(message, "attachments", []) or []
            if attachments:
                # Prefer listing URLs if short; otherwise show a count
                att_urls = ", ".join(getattr(a, 'url', str(a)) for a in attachments)
                if len(att_urls) <= MAX_FIELD_LENGTH:
                    embed.add_field(name="Attachments", value=att_urls, inline=False)
                else:
                    embed.add_field(name="Attachments", value=f"{len(attachments)} attachment(s)", inline=False)
        except Exception:
            # Ignore attachment formatting failures
            pass

        # Footer with author/id and message timestamp
        footer_text = _format_footer(getattr(author, 'id', None), getattr(message, 'id', None), getattr(message, 'created_at', None))
        if footer_text:
            embed.set_footer(text=footer_text)
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_bulk_message_delete(self, messages: List[discord.Message]):
        if not messages:
            return
        guild = messages[0].guild
        if guild is None:
            return
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        title = "🗑️ Bulk Message Delete"
        chans = {}
        for m in messages:
            ch = getattr(m.channel, "id", None)
            chans.setdefault(ch, 0)
            chans[ch] += 1
        lines = [f"Total messages deleted: {len(messages)}"]
        for ch_id, cnt in chans.items():
            lines.append(f"Channel ID {ch_id}: {cnt} messages")
        embed = self._build_basic_embed(title, discord.Color.dark_grey(), "\n".join(lines))
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_message_edit(self, before: discord.Message, after: discord.Message):
        if before.content == after.content:
            return
        guild = after.guild
        if guild is None:
            return
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        title = "✍️ Message Edited"
        # Keep short identifying info in the description
        desc_parts = [f"Channel: {getattr(after.channel, 'name', repr(after.channel))} (ID {getattr(after.channel, 'id', 'unknown')})"]
        desc_parts.append(f"Author: {getattr(after.author, 'name', repr(after.author))} (ID {getattr(after.author, 'id', 'unknown')})")
        desc_parts.append(f"Message ID: {after.id}")
        embed = self._build_basic_embed(title, discord.Color.dark_blue(), "\n".join(desc_parts))

        # Add Before/After as dedicated fields (non-inline) so they render cleanly
        before_content = _truncate(getattr(before, 'content', None)) or "(unavailable)"
        after_content = _truncate(getattr(after, 'content', None)) or "(unavailable)"
        try:
            embed.add_field(name="Before", value=before_content, inline=False)
        except Exception:
            # If adding a field fails for any reason, append to description as fallback
            embed.description = (embed.description or "") + "\nBefore: " + before_content
        try:
            embed.add_field(name="After", value=after_content, inline=False)
        except Exception:
            embed.description = (embed.description or "") + "\nAfter: " + after_content

        # Footer with author/id and message timestamp
        footer_text = _format_footer(getattr(after.author, 'id', None), getattr(after, 'id', None), getattr(after, 'created_at', None))
        if footer_text:
            embed.set_footer(text=footer_text)
        await self._safe_send(channel, embed=embed)

    @commands.Cog.listener()
    async def on_guild_channel_create(self, channel: discord.abc.GuildChannel):
        guild = channel.guild
        ch = await self._resolve_channel(guild)
        if ch is None:
            return
        actor, reason = await self._fetch_audit_actor(guild, discord.AuditLogAction.channel_create, target_id=channel.id)
        title = "📁 Channel Created"
        desc = f"Channel: {channel.name} (ID {channel.id}) Type: {type(channel).__name__}"
        if actor:
            desc += f" by {actor} (ID {actor.id})"
        if reason:
            desc += f" — Reason: {reason}"
        embed = self._build_basic_embed(title, discord.Color.teal(), desc)
        await self._safe_send(ch, embed=embed)

    @commands.Cog.listener()
    async def on_guild_channel_delete(self, channel: discord.abc.GuildChannel):
        guild = channel.guild
        ch = await self._resolve_channel(guild)
        if ch is None:
            return
        actor, reason = await self._fetch_audit_actor(guild, discord.AuditLogAction.channel_delete, target_id=channel.id)
        title = "🗑️ Channel Deleted"
        desc = f"Channel: {getattr(channel, 'name', '(deleted)')} (ID {channel.id})"
        if actor:
            desc += f" by {actor} (ID {actor.id})"
        if reason:
            desc += f" — Reason: {reason}"
        embed = self._build_basic_embed(title, discord.Color.dark_grey(), desc)
        await self._safe_send(ch, embed=embed)

    @commands.Cog.listener()
    async def on_guild_channel_update(self, before: discord.abc.GuildChannel, after: discord.abc.GuildChannel):
        guild = after.guild
        ch = await self._resolve_channel(guild)
        if ch is None:
            return
        actor, reason = await self._fetch_audit_actor(guild, discord.AuditLogAction.channel_update, target_id=after.id)
        title = "✏️ Channel Edited"
        diffs = []
        try:
            if getattr(before, 'name', None) != getattr(after, 'name', None):
                diffs.append(f"Name: '{getattr(before, 'name', None)}' -> '{getattr(after, 'name', None)}'")
            if getattr(before, 'position', None) != getattr(after, 'position', None):
                diffs.append(f"Position: {getattr(before, 'position', None)} -> {getattr(after, 'position', None)}")
        except Exception:
            diffs.append("Channel properties changed")
        if not diffs:
            diffs.append("No visible changes")
        desc = f"Channel: {getattr(after, 'name', repr(after))} (ID {after.id})\n" + "; ".join(diffs)
        if actor:
            desc += f"\nEdited by: {actor} (ID {actor.id})"
        if reason:
            desc += f"\nReason: {reason}"
        embed = self._build_basic_embed(title, discord.Color.gold(), desc)
        await self._safe_send(ch, embed=embed)

    @commands.Cog.listener()
    async def on_voice_state_update(self, member: discord.Member, before: discord.VoiceState, after: discord.VoiceState):
        guild = member.guild
        channel = await self._resolve_channel(guild)
        if channel is None:
            return
        # Determine join/leave/move
        b = getattr(before, 'channel', None)
        a = getattr(after, 'channel', None)
        if b is None and a is not None:
            title = "🔊 Voice Channel Join"
            desc = f"Member: {member} joined {a.name} (ID {a.id})"
        elif b is not None and a is None:
            title = "🔇 Voice Channel Leave"
            desc = f"Member: {member} left {b.name} (ID {b.id})"
        elif b is not None and a is not None and b.id != a.id:
            title = "🔀 Voice Channel Move"
            desc = f"Member: {member} moved {b.name} (ID {b.id}) -> {a.name} (ID {a.id})"
        else:
            return
        embed = self._build_basic_embed(title, discord.Color.blurple(), desc)
        await self._safe_send(channel, embed=embed)


async def setup(bot: commands.Bot):
    await bot.add_cog(DiscordLogger(bot))

import logging
import discord
from discord.ext import commands
from typing import Any, Dict, List


class DiscordLogger(commands.Cog):
    """Cog that logs deleted messages to the project's logger.

    Features:
    - on_message_delete & on_raw_message_delete
    - on_member_update -> logs role additions/removals and profile changes
    - on_guild_role_update / on_guild_role_create / on_guild_role_delete -> logs role changes
    - on_guild_channel_update / on_guild_channel_create / on_guild_channel_delete -> logs channel changes
    - on_voice_state_update, on_guild_emojis_update, on_invite_create/delete, on_webhooks_update

    Notes:
    - Intents: member and guild intents are required (your bot already sets Intents.all()).
    """

    def __init__(self, bot: commands.Bot, *, skip_bots: bool = True, max_content_len: int = 400):
        self.bot = bot
        # Use a dedicated logger name so entries can be filtered or routed separately if desired.
        self.logger = logging.getLogger("deleted_messages")
        self.server_logger = logging.getLogger("server_events")
        self.skip_bots = skip_bots
        self.max_content_len = max_content_len

    def _truncate(self, text: str) -> str:
        if text is None:
            return ""
        if len(text) > self.max_content_len:
            return text[: self.max_content_len] + "...(truncated)"
        return text

    @commands.Cog.listener()
    async def on_message_delete(self, message: discord.Message):
        """Called when a Message is deleted and the bot had it cached.

        We'll log author (id/name), channel (id/name), guild id, message id, content (truncated),
        attachments, and a short embeds representation.
        """
        try:
            if self.skip_bots and message.author and message.author.bot:
                return

            content = getattr(message, "content", None)
            # prefer clean_content when available (sanitizes mentions)
            try:
                content = message.clean_content
            except Exception:
                # Fallback to raw content
                content = content or ""

            content = self._truncate(content)

            attachments = [a.url for a in (message.attachments or [])]
            embeds = [e.to_dict() for e in (message.embeds or [])]

            # Log as structured extra fields so JSON formatter can pick them up if configured.
            self.logger.info(
                "message_deleted",
                extra={
                    "event": "message_deleted",
                    "message_id": getattr(message, "id", None),
                    "channel_id": getattr(getattr(message, "channel", None), "id", None),
                    "channel_name": getattr(getattr(message, "channel", None), "name", None),
                    "guild_id": getattr(message, "guild", None) and getattr(message.guild, "id", None),
                    "author_id": getattr(getattr(message, "author", None), "id", None),
                    "author_name": getattr(getattr(message, "author", None), "name", None),
                    "content": content,
                    "attachments": attachments,
                    "embeds": embeds,
                },
            )
        except Exception:
            # Ensure we never crash the bot because of logging
            self.logger.exception("Failed to handle on_message_delete")

    @commands.Cog.listener()
    async def on_raw_message_delete(self, payload: discord.RawMessageDeleteEvent):
        """Called when a message is deleted but it wasn't in the bot's message cache.

        payload contains message_id and channel_id (and guild_id when applicable).
        Content is not available in this event.
        """
        try:
            # Raw events are lower-overhead; don't filter bots here because we don't have the author.
            self.logger.info(
                "raw_message_deleted",
                extra={
                    "event": "raw_message_deleted",
                    "message_id": getattr(payload, "message_id", None),
                    "channel_id": getattr(payload, "channel_id", None),
                    "guild_id": getattr(payload, "guild_id", None),
                },
            )
        except Exception:
            self.logger.exception("Failed to handle on_raw_message_delete")

    def _object_to_dict(self, obj: Any) -> Dict[str, Any]:
        """Small helper to represent common discord objects as serializable dicts."""
        if obj is None:
            return {}
        # Roles
        if isinstance(obj, discord.Role):
            return {"id": obj.id, "name": obj.name, "colour": obj.colour.value, "permissions": obj.permissions.value}
        # Members
        if isinstance(obj, discord.Member):
            return {"id": obj.id, "name": obj.display_name, "roles": [r.id for r in obj.roles]}
        # Channels
        if isinstance(obj, discord.abc.GuildChannel):
            return {"id": obj.id, "name": getattr(obj, "name", None), "type": str(obj.type)}
        # PermissionOverwrite - best-effort to extract allow/deny values
        if isinstance(obj, discord.PermissionOverwrite):
            allow_val = None
            deny_val = None
            try:
                # some PermissionOverwrite-like objects expose pair()
                if hasattr(obj, "pair") and callable(getattr(obj, "pair")):
                    a, d = obj.pair()
                    allow_val = getattr(a, "value", a)
                    deny_val = getattr(d, "value", d)
                else:
                    # fallback: try attributes
                    a = getattr(obj, "allow", None)
                    d = getattr(obj, "deny", None)
                    allow_val = getattr(a, "value", a)
                    deny_val = getattr(d, "value", d)
            except Exception:
                allow_val = None
                deny_val = None
            return {"allow": allow_val, "deny": deny_val}
        # Fallback
        return {"repr": str(obj)}

    async def _find_audit_actor(self, guild: discord.Guild, action: Any, target_id: int = None) -> Dict[str, Any] | None:
        """Try to find a recent audit log entry matching the action and target id.

        Returns a small dict with user id/name and the entry id, or None.
        This is a best-effort helper and may return None if audit logs are unavailable or the entry isn't found.
        """
        try:
            if guild is None:
                return None
            async for entry in guild.audit_logs(limit=12, action=action):
                try:
                    t = getattr(entry.target, "id", None)
                except Exception:
                    t = None
                if target_id is None or t == target_id:
                    user = entry.user
                    return {"moderator_id": getattr(user, "id", None), "moderator_name": getattr(user, "name", None), "entry_id": getattr(entry, "id", None)}
        except Exception:
            # don't propagate audit log errors
            self.server_logger.debug("Audit log lookup failed", exc_info=True)
        return None

    @commands.Cog.listener()
    async def on_member_update(self, before: discord.Member, after: discord.Member):
        """Detect role additions/removals and profile changes on a member."""
        try:
            # roles is a list with @everyone at index 0; compute set diffs
            before_roles = {r.id for r in before.roles}
            after_roles = {r.id for r in after.roles}

            added = after_roles - before_roles
            removed = before_roles - after_roles

            if added or removed:
                self.server_logger.info(
                    "member_roles_changed",
                    extra={
                        "event": "member_roles_changed",
                        "guild_id": getattr(after.guild, "id", None),
                        "member_id": after.id,
                        "member_name": after.display_name,
                        "roles_added": list(added),
                        "roles_removed": list(removed),
                    },
                )

            # Detect nickname/display name change
            before_name = getattr(before, "display_name", None)
            after_name = getattr(after, "display_name", None)
            if before_name != after_name:
                self.server_logger.info(
                    "member_name_changed",
                    extra={
                        "event": "member_name_changed",
                        "guild_id": getattr(after.guild, "id", None),
                        "member_id": after.id,
                        "before": before_name,
                        "after": after_name,
                    },
                )

            # Avatar change
            before_avatar = getattr(before, "avatar", None)
            after_avatar = getattr(after, "avatar", None)
            if before_avatar != after_avatar:
                self.server_logger.info(
                    "member_avatar_changed",
                    extra={
                        "event": "member_avatar_changed",
                        "guild_id": getattr(after.guild, "id", None),
                        "member_id": after.id,
                        "avatar_before": str(before_avatar),
                        "avatar_after": str(after_avatar),
                    },
                )
        except Exception:
            self.server_logger.exception("Failed to handle on_member_update")

    @commands.Cog.listener()
    async def on_voice_state_update(self, member: discord.Member, before: discord.VoiceState, after: discord.VoiceState):
        try:
            # Log joins/leaves/mute/deaf changes
            if before.channel != after.channel:
                self.server_logger.info(
                    "voice_channel_move",
                    extra={
                        "event": "voice_channel_move",
                        "guild_id": getattr(member.guild, "id", None),
                        "member_id": getattr(member, "id", None),
                        "channel_before": getattr(getattr(before, "channel", None), "id", None),
                        "channel_after": getattr(getattr(after, "channel", None), "id", None),
                    },
                )

            # Mute/deaf status changes
            if before.mute != after.mute or before.deaf != after.deaf or before.self_mute != after.self_mute or before.self_deaf != after.self_deaf:
                self.server_logger.info(
                    "voice_state_changed",
                    extra={
                        "event": "voice_state_changed",
                        "guild_id": getattr(member.guild, "id", None),
                        "member_id": getattr(member, "id", None),
                        "mute_before": getattr(before, "mute", None),
                        "mute_after": getattr(after, "mute", None),
                        "deaf_before": getattr(before, "deaf", None),
                        "deaf_after": getattr(after, "deaf", None),
                        "self_mute_before": getattr(before, "self_mute", None),
                        "self_mute_after": getattr(after, "self_mute", None),
                        "self_deaf_before": getattr(before, "self_deaf", None),
                        "self_deaf_after": getattr(after, "self_deaf", None),
                    },
                )
        except Exception:
            self.server_logger.exception("Failed to handle on_voice_state_update")

    @commands.Cog.listener()
    async def on_guild_emojis_update(self, guild: discord.Guild, before: List[discord.Emoji], after: List[discord.Emoji]):
        try:
            before_ids = {e.id for e in before}
            after_ids = {e.id for e in after}
            added = after_ids - before_ids
            removed = before_ids - after_ids
            if not added and not removed:
                return
            self.server_logger.info(
                "emojis_changed",
                extra={
                    "event": "emojis_changed",
                    "guild_id": getattr(guild, "id", None),
                    "emojis_added": list(added),
                    "emojis_removed": list(removed),
                },
            )
        except Exception:
            self.server_logger.exception("Failed to handle on_guild_emojis_update")

    @commands.Cog.listener()
    async def on_invite_create(self, invite: discord.Invite):
        try:
            self.server_logger.info(
                "invite_created",
                extra={
                    "event": "invite_created",
                    "guild_id": getattr(getattr(invite, "guild", None), "id", None),
                    "channel_id": getattr(getattr(invite, "channel", None), "id", None),
                    "code": getattr(invite, "code", None),
                    "inviter_id": getattr(getattr(invite, "inviter", None), "id", None),
                    "max_uses": getattr(invite, "max_uses", None),
                    "temporary": getattr(invite, "temporary", None),
                },
            )
        except Exception:
            self.server_logger.exception("Failed to handle on_invite_create")

    @commands.Cog.listener()
    async def on_invite_delete(self, invite: discord.Invite):
        try:
            self.server_logger.info(
                "invite_deleted",
                extra={
                    "event": "invite_deleted",
                    "guild_id": getattr(getattr(invite, "guild", None), "id", None),
                    "channel_id": getattr(getattr(invite, "channel", None), "id", None),
                    "code": getattr(invite, "code", None),
                },
            )
        except Exception:
            self.server_logger.exception("Failed to handle on_invite_delete")

    @commands.Cog.listener()
    async def on_webhooks_update(self, channel: discord.abc.GuildChannel):
        try:
            self.server_logger.info(
                "webhooks_updated",
                extra={
                    "event": "webhooks_updated",
                    "guild_id": getattr(getattr(channel, "guild", None), "id", None),
                    "channel_id": getattr(channel, "id", None),
                    "channel_name": getattr(channel, "name", None),
                },
            )
        except Exception:
            self.server_logger.exception("Failed to handle on_webhooks_update")

    # Role/channel event handlers already implemented earlier — attach audit actors where possible
    @commands.Cog.listener()
    async def on_guild_role_create(self, role: discord.Role):
        try:
            guild = getattr(role, "guild", None)
            actor = await self._find_audit_actor(guild, discord.AuditLogAction.role_create, target_id=getattr(role, "id", None))
            extra = {
                "event": "role_created",
                "guild_id": getattr(guild, "id", None),
                "role_id": role.id,
                "role_name": role.name,
                "permissions": role.permissions.value,
                "colour": role.colour.value,
            }
            if actor:
                extra.update(actor)
            self.server_logger.info("role_created", extra=extra)
        except Exception:
            self.server_logger.exception("Failed to handle on_guild_role_create")

    @commands.Cog.listener()
    async def on_guild_role_update(self, before: discord.Role, after: discord.Role):
        """Log edits to a role's properties (name, colour, permissions, hoist, mentionable, position)."""
        try:
            # Diagnostic log to confirm event fires and inspect values (will show in server_events.jsonl).
            try:
                self.server_logger.info(
                    "role_update_event",
                    extra={
                        "event": "role_update_event",
                        "guild_id": getattr(after, "guild", None) and getattr(after.guild, "id", None),
                        "role_id": getattr(after, "id", None),
                        "before_name": getattr(before, "name", None),
                        "after_name": getattr(after, "name", None),
                        "before_colour": getattr(getattr(before, "colour", None), "value", None),
                        "after_colour": getattr(getattr(after, "colour", None), "value", None),
                        "before_permissions": getattr(getattr(before, "permissions", None), "value", None),
                        "after_permissions": getattr(getattr(after, "permissions", None), "value", None),
                        "before_repr": repr(before),
                        "after_repr": repr(after),
                    },
                )
            except Exception:
                # non-fatal for diagnostics
                self.server_logger.debug("Failed to write diagnostic role_update_event", exc_info=True)

            guild = getattr(after, "guild", None) or getattr(before, "guild", None)
            diffs: Dict[str, Any] = {}
            if before.name != after.name:
                diffs["name"] = {"before": before.name, "after": after.name}
            if before.colour != after.colour:
                diffs["colour"] = {"before": getattr(before.colour, "value", None), "after": getattr(after.colour, "value", None)}
            if before.permissions != after.permissions:
                diffs["permissions"] = {"before": before.permissions.value, "after": after.permissions.value}
            if getattr(before, "hoist", None) != getattr(after, "hoist", None):
                diffs["hoist"] = {"before": getattr(before, "hoist", None), "after": getattr(after, "hoist", None)}
            if getattr(before, "mentionable", None) != getattr(after, "mentionable", None):
                diffs["mentionable"] = {"before": getattr(before, "mentionable", None), "after": getattr(after, "mentionable", None)}
            # position can change when roles are reordered
            if getattr(before, "position", None) != getattr(after, "position", None):
                diffs["position"] = {"before": getattr(before, "position", None), "after": getattr(after, "position", None)}

            if not diffs:
                return

            actor = None
            try:
                actor = await self._find_audit_actor(guild, discord.AuditLogAction.role_update, target_id=getattr(after, "id", None))
            except Exception:
                actor = None

            extra = {
                "event": "role_updated",
                "guild_id": getattr(guild, "id", None),
                "role_id": getattr(after, "id", None),
                "role_name_before": getattr(before, "name", None),
                "role_name_after": getattr(after, "name", None),
                "diffs": diffs,
            }
            if actor:
                extra.update(actor)

            self.server_logger.info("role_updated", extra=extra)
        except Exception:
            self.server_logger.exception("Failed to handle on_guild_role_update")

    @commands.Cog.listener()
    async def on_guild_role_delete(self, role: discord.Role):
        try:
            guild = getattr(role, "guild", None)
            actor = await self._find_audit_actor(guild, discord.AuditLogAction.role_delete, target_id=getattr(role, "id", None))
            extra = {
                "event": "role_deleted",
                "guild_id": getattr(guild, "id", None),
                "role_id": role.id,
                "role_name": role.name,
            }
            if actor:
                extra.update(actor)
            self.server_logger.info("role_deleted", extra=extra)
        except Exception:
            self.server_logger.exception("Failed to handle on_guild_role_delete")

    @commands.Cog.listener()
    async def on_guild_channel_create(self, channel: discord.abc.GuildChannel):
        try:
            actor = None
            try:
                actor = await self._find_audit_actor(channel.guild, discord.AuditLogAction.channel_create, target_id=getattr(channel, "id", None))
            except Exception:
                actor = None
            extra = {
                "event": "channel_created",
                "guild_id": getattr(channel.guild, "id", None),
                "channel_id": getattr(channel, "id", None),
                "channel_name": getattr(channel, "name", None),
                "type": str(getattr(channel, "type", None)),
            }
            if actor:
                extra.update(actor)
            self.server_logger.info("channel_created", extra=extra)
        except Exception:
            self.server_logger.exception("Failed to handle on_guild_channel_create")

    @commands.Cog.listener()
    async def on_guild_channel_delete(self, channel: discord.abc.GuildChannel):
        try:
            actor = None
            try:
                actor = await self._find_audit_actor(channel.guild, discord.AuditLogAction.channel_delete, target_id=getattr(channel, "id", None))
            except Exception:
                actor = None
            extra = {
                "event": "channel_deleted",
                "guild_id": getattr(channel.guild, "id", None),
                "channel_id": getattr(channel, "id", None),
                "channel_name": getattr(channel, "name", None),
            }
            if actor:
                extra.update(actor)
            self.server_logger.info("channel_deleted", extra=extra)
        except Exception:
            self.server_logger.exception("Failed to handle on_guild_channel_delete")

    @commands.Cog.listener()
    async def on_guild_channel_update(self, before: discord.abc.GuildChannel, after: discord.abc.GuildChannel):
        try:
            diffs: Dict[str, Any] = {}
            if getattr(before, "name", None) != getattr(after, "name", None):
                diffs["name"] = {"before": getattr(before, "name", None), "after": getattr(after, "name", None)}
            if getattr(before, "type", None) != getattr(after, "type", None):
                diffs["type"] = {"before": str(getattr(before, "type", None)), "after": str(getattr(after, "type", None))}

            # Permission overwrites can change — do a best-effort comparison by overwrites' ids
            before_overwrites = { (getattr(o, 'id', None) if hasattr(o, 'id') else None): o for o in getattr(before, '_overwrites', []) }
            after_overwrites = { (getattr(o, 'id', None) if hasattr(o, 'id') else None): o for o in getattr(after, '_overwrites', []) }
            if set(before_overwrites.keys()) != set(after_overwrites.keys()):
                diffs["permission_overwrite_ids"] = {"before": list(before_overwrites.keys()), "after": list(after_overwrites.keys())}

            if not diffs:
                return

            actor = None
            try:
                actor = await self._find_audit_actor(getattr(after, 'guild', None), discord.AuditLogAction.channel_update, target_id=getattr(after, 'id', None))
            except Exception:
                actor = None

            extra = {
                "event": "channel_updated",
                "guild_id": getattr(after.guild, "id", None),
                "channel_id": getattr(after, "id", None),
                "channel_name": getattr(after, "name", None),
                "diffs": diffs,
            }
            if actor:
                extra.update(actor)

            self.server_logger.info("channel_updated", extra=extra)
        except Exception:
            self.server_logger.exception("Failed to handle on_guild_channel_update")

    @commands.Cog.listener()
    async def on_message_edit(self, before: discord.Message, after: discord.Message):
        """Log message edits when available (cached)."""
        try:
            if self.skip_bots and before.author and before.author.bot:
                return
            if before.content == after.content:
                return
            self.server_logger.info(
                "message_edited",
                extra={
                    "event": "message_edited",
                    "message_id": getattr(before, "id", None),
                    "channel_id": getattr(getattr(before, "channel", None), "id", None),
                    "guild_id": getattr(before.guild, "id", None),
                    "author_id": getattr(getattr(before, "author", None), "id", None),
                    "author_name": getattr(getattr(before, "author", None), "name", None),
                    "before": self._truncate(getattr(before, "content", "")),
                    "after": self._truncate(getattr(after, "content", "")),
                },
            )
        except Exception:
            self.server_logger.exception("Failed to handle on_message_edit")

    @commands.Cog.listener()
    async def on_member_join(self, member: discord.Member):
        try:
            self.server_logger.info(
                "member_join",
                extra={
                    "event": "member_join",
                    "guild_id": getattr(member.guild, "id", None),
                    "member_id": getattr(member, "id", None),
                    "member_name": getattr(member, "name", None) or getattr(member, "display_name", None),
                },
            )
        except Exception:
            self.server_logger.exception("Failed to handle on_member_join")

    @commands.Cog.listener()
    async def on_member_remove(self, member: discord.Member):
        try:
            self.server_logger.info(
                "member_remove",
                extra={
                    "event": "member_remove",
                    "guild_id": getattr(member.guild, "id", None),
                    "member_id": getattr(member, "id", None),
                    "member_name": getattr(member, "name", None) or getattr(member, "display_name", None),
                },
            )
        except Exception:
            self.server_logger.exception("Failed to handle on_member_remove")

    @commands.Cog.listener()
    async def on_member_ban(self, guild: discord.Guild, user: discord.User):
        try:
            actor = await self._find_audit_actor(guild, discord.AuditLogAction.ban, target_id=getattr(user, "id", None))
            extra = {
                "event": "member_ban",
                "guild_id": getattr(guild, "id", None),
                "user_id": getattr(user, "id", None),
                "user_name": getattr(user, "name", None),
            }
            if actor:
                extra.update(actor)
            self.server_logger.info("member_ban", extra=extra)
        except Exception:
            self.server_logger.exception("Failed to handle on_member_ban")

    @commands.Cog.listener()
    async def on_member_unban(self, guild: discord.Guild, user: discord.User):
        try:
            actor = await self._find_audit_actor(guild, discord.AuditLogAction.unban, target_id=getattr(user, "id", None))
            extra = {
                "event": "member_unban",
                "guild_id": getattr(guild, "id", None),
                "user_id": getattr(user, "id", None),
                "user_name": getattr(user, "name", None),
            }
            if actor:
                extra.update(actor)
            self.server_logger.info("member_unban", extra=extra)
        except Exception:
            self.server_logger.exception("Failed to handle on_member_unban")

    @commands.Cog.listener()
    async def on_guild_update(self, before: discord.Guild, after: discord.Guild):
        try:
            diffs: Dict[str, Any] = {}
            if before.name != after.name:
                diffs["name"] = {"before": before.name, "after": after.name}
            if getattr(before, "description", None) != getattr(after, "description", None):
                diffs["description"] = {"before": getattr(before, "description", None), "after": getattr(after, "description", None)}
            if not diffs:
                return
            self.server_logger.info(
                "guild_updated",
                extra={
                    "event": "guild_updated",
                    "guild_id": getattr(after, "id", None),
                    "diffs": diffs,
                },
            )
        except Exception:
            self.server_logger.exception("Failed to handle on_guild_update")

    @commands.command(name="roleinfo")
    @commands.has_guild_permissions(administrator=True)
    async def roleinfo(self, ctx: commands.Context, role: discord.Role):
        """Admin command: log a snapshot of a role's current properties to server_events.jsonl."""
        try:
            guild = getattr(role, "guild", None)
            extra = {
                "event": "role_snapshot",
                "guild_id": getattr(guild, "id", None),
                "role_id": role.id,
                "role_name": role.name,
                "permissions": getattr(getattr(role, "permissions", None), "value", None),
                "colour": getattr(getattr(role, "colour", None), "value", None),
                "hoist": getattr(role, "hoist", None),
                "mentionable": getattr(role, "mentionable", None),
                "position": getattr(role, "position", None),
            }
            self.server_logger.info("role_snapshot", extra=extra)
            await ctx.send(f"Logged snapshot for role `{role.name}` ({role.id}).")
        except Exception as e:
            self.server_logger.exception("Failed to run roleinfo command")
            await ctx.send(f"Failed to log role info: {e}")

    @commands.command(name="force_role_snapshot")
    @commands.has_guild_permissions(administrator=True)
    async def force_role_snapshot(self, ctx: commands.Context, role: discord.Role):
        """Admin command: force a role snapshot (same as roleinfo) to help debugging."""
        await self.roleinfo.callback(self, ctx, role)


async def setup(bot: commands.Bot):
    """Extension entrypoint for discord.py v2-style extensions."""
    await bot.add_cog(DiscordLogger(bot))

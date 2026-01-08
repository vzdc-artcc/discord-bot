# python
from flask import Blueprint, request, jsonify
from extensions.api_server import app, api_key_required
import discord
import re
import config as cfg
import logging

bp = Blueprint("create_training_channel", __name__, url_prefix="/create_training_channel")

logger = logging.getLogger(__name__)


def _slugify(s: str) -> str:
    s = (s or "").strip().lower()
    # Remove any characters that aren't word chars, spaces or hyphens
    s = re.sub(r"[^\w\s-]", "", s)
    # Replace whitespace/underscores with single hyphen
    s = re.sub(r"[\s_]+", "-", s)
    return s


@bp.route("", methods=["POST"])
@api_key_required
def create_training_channel():
    """Endpoint to create a per-student training channel.

    Expects JSON body containing `student`, `primaryTrainer` and optional `otherTrainers` as in the incoming webhook.
    The student must have `discordUid`, `firstName`, `lastName`, `cid`.

    The route will find the guild where the student is a member, create a channel named
    `firstname-lastname-cid` under that guild's configured training category, and set permission
    overwrites so only the student and trainers can view/send messages.
    """

    data = request.get_json(silent=True)
    logger.debug("create_training_channel called")
    logger.info("Request JSON payload: %s", data)

    if not data:
        logger.warning("Invalid or missing JSON body")
        return jsonify({"error": "Invalid or missing JSON body"}), 400

    student = data.get("student")
    primary = data.get("primaryTrainer")
    others = data.get("otherTrainers") or []

    # Optional: client may include the target guild id directly in the request body
    guild_id_raw = data.get("guild_id")
    guild_id = None
    if guild_id_raw is not None:
        try:
            guild_id = int(guild_id_raw)
        except Exception:
            logger.warning("guild_id provided but is not an integer: %r", guild_id_raw)
            return jsonify({"error": "guild_id must be an integer"}), 400

    # Basic validation
    if not student or not primary:
        logger.warning("Missing required student or primaryTrainer object")
        return jsonify({"error": "Missing required student or primaryTrainer object"}), 400

    try:
        student_uid = int(student.get("discordUid"))
    except Exception:
        logger.warning("student.discordUid is required and must be an integer: %r", student.get("discordUid"))
        return jsonify({"error": "student.discordUid is required and must be an integer"}), 400

    try:
        primary_uid = int(primary.get("discordUid"))
    except Exception:
        logger.debug("primaryTrainer.discordUid missing or invalid; continuing without resolved primary member")
        primary_uid = None

    other_uids = []
    for o in others:
        try:
            other_uids.append(int(o.get("discordUid")))
        except Exception:
            logger.debug("Skipping otherTrainer with invalid discordUid: %r", o)
            continue

    first = student.get("firstName") or ""
    last = student.get("lastName") or ""
    cid = student.get("cid") or ""

    if not (first and last and cid):
        logger.warning("Missing required student name/cid fields: first=%r last=%r cid=%r", first, last, cid)
        return jsonify({"error": "student.firstName, student.lastName and student.cid are required"}), 400

    raw_name = f"{first}-{last}-{cid}"
    channel_name = _slugify(raw_name)
    logger.debug("Computed channel name: %s (raw=%s)", channel_name, raw_name)

    async def _op():
        bot = getattr(app, "bot", None)
        if bot is None:
            logger.error("Discord bot instance not available on Flask app")
            raise RuntimeError("Discord bot instance not available on Flask app")

        # Find guild where the student is present. If a guild_id was provided in the
        # request body, prefer resolving that guild directly and ensure the student
        # is present in it. Otherwise, search all guilds the bot is in for the student.
        target_guild = None
        student_member = None
        if guild_id is not None:
            logger.debug("guild_id provided in request; resolving guild %s", guild_id)
            # Prefer bot.get_guild if present (discord.Client/commands.Bot)
            target_guild = bot.get_guild(guild_id) if hasattr(bot, "get_guild") else next((gg for gg in bot.guilds if gg.id == guild_id), None)
            if target_guild is None:
                logger.error("Guild id %s provided but bot is not a member of that guild", guild_id)
                raise RuntimeError(f"Guild id {guild_id} not found in bot.guilds")

            # Try to find the student in the provided guild
            m = target_guild.get_member(student_uid)
            if m is None:
                try:
                    m = await target_guild.fetch_member(student_uid)
                except Exception:
                    m = None
            if m:
                student_member = m
                logger.info("Found student %s in provided guild %s", student_uid, target_guild.id)
            else:
                logger.error("Student with discord id %s not found in provided guild %s", student_uid, target_guild.id)
                raise RuntimeError(f"Student with discord id {student_uid} not found in guild {target_guild.id}")
        else:
            logger.debug("Searching bot.guilds (%d) for member %s", len(bot.guilds), student_uid)
            for g in bot.guilds:
                logger.debug("Checking guild id=%s name=%s for member %s", g.id, getattr(g, "name", "<no-name>"), student_uid)
                # Try cached member first
                m = g.get_member(student_uid)
                if m is None:
                    try:
                        m = await g.fetch_member(student_uid)
                    except Exception:
                        m = None
                if m:
                    target_guild = g
                    student_member = m
                    logger.info("Found student %s in guild %s", student_uid, target_guild.id)
                    break

        if target_guild is None:
            logger.error("Student with discord id %s not found in any guild the bot is in", student_uid)
            raise RuntimeError(f"Student with discord id {student_uid} not found in any guild the bot is in")

        # Resolve primary trainer and other trainers to Member objects if possible
        async def _resolve_member(guild, uid):
            if uid is None:
                return None
            m = guild.get_member(uid)
            if m is None:
                try:
                    m = await guild.fetch_member(uid)
                except Exception:
                    m = None
            return m

        primary_member = await _resolve_member(target_guild, primary_uid)
        if primary_member:
            logger.info("Resolved primary trainer to member id=%s display=%s", primary_member.id, getattr(primary_member, "display_name", None))
        else:
            logger.debug("Primary trainer not resolvable to guild member; will mention by id if provided")

        other_members = []
        for uid in other_uids:
            m = await _resolve_member(target_guild, uid)
            if m:
                logger.debug("Resolved other trainer uid %s to member id=%s", uid, m.id)
                other_members.append(m)
            else:
                logger.debug("Could not resolve other trainer uid %s to a guild member", uid)

        # Determine category id from guild config (custom key 'categories.training_channels_category_id')
        guild_cfg = cfg.get_guild_config(target_guild.id)
        categories = guild_cfg.as_dict().get("categories") or {}
        cat_id = categories.get("training_channels_category_id")
        category_obj = None
        if cat_id:
            logger.info("Guild %s configured training category id=%s", target_guild.id, cat_id)
            try:
                # Prefer resolving the category from the target guild to avoid cross-guild mismatches
                category_obj = target_guild.get_channel(int(cat_id))
                if category_obj is None:
                    # Fallback to fetching the channel globally and ensure it belongs to this guild
                    category_obj = await bot.fetch_channel(int(cat_id))
                    if getattr(category_obj, 'guild', None) != target_guild:
                        logger.warning("Configured category id %s does not belong to guild %s", cat_id, target_guild.id)
                        category_obj = None
                # Ensure the resolved object is actually a CategoryChannel
                if category_obj is not None and not isinstance(category_obj, discord.CategoryChannel):
                    logger.warning("Configured category id %s resolved to non-category channel type", cat_id)
                    category_obj = None
                if category_obj is not None:
                    logger.debug("Resolved category object id=%s name=%s", category_obj.id, getattr(category_obj, "name", None))
            except Exception:
                logger.exception("Failed to resolve configured category id %s", cat_id)
                category_obj = None
        else:
            logger.info("No training category configured for guild %s; creating channel at guild root", target_guild.id)

        # Ensure channel doesn't already exist
        existing = discord.utils.get(target_guild.channels, name=channel_name)
        if existing is not None:
            logger.info("Channel already exists: name=%s id=%s guild=%s", channel_name, existing.id, target_guild.id)
            return {"status": "exists", "channel_id": existing.id, "guild_id": target_guild.id}

        # Build permission overwrites
        overwrites = {}
        # Deny @everyone view
        overwrites[target_guild.default_role] = discord.PermissionOverwrite(view_channel=False)

        def allow_member_perms(member):
            return discord.PermissionOverwrite(view_channel=True, send_messages=True, read_message_history=True)

        overwrites[student_member] = allow_member_perms(student_member)
        if primary_member:
            overwrites[primary_member] = allow_member_perms(primary_member)
        for m in other_members:
            overwrites[m] = allow_member_perms(m)

        # Create channel under the category if available
        # Provide a reason so this action is visible in audit logs and can be filtered by the logger
        logger.info("Creating channel %s in guild %s under category %s", channel_name, target_guild.id, getattr(category_obj, "id", None))
        channel = await target_guild.create_text_channel(channel_name, overwrites=overwrites, category=category_obj, reason="create_training_channel (API)")
        logger.info("Created channel %s (id=%s) in guild %s", channel.name, channel.id, target_guild.id)

        try:
            trainer_mentions = []
            if primary_member:
                trainer_mentions.append(primary_member.mention)
            elif primary_uid is not None:
                trainer_mentions.append(f"<@{primary_uid}>")

            for uid in other_uids:
                # try to find resolved member in other_members matching uid
                m_obj = None
                for m in other_members:
                    if getattr(m, 'id', None) == int(uid):
                        m_obj = m
                        break
                trainer_mentions.append(m_obj.mention if m_obj else f"<@{uid}>")

            trainers_text = ", ".join(trainer_mentions) if trainer_mentions else "(no trainers specified)"

            # Student mention (greet) - prefer Member.mention
            student_mention = student_member.mention if student_member else f"<@{student_uid}>"

            # Build a minimal embed with only the requested content
            embed = discord.Embed(title="Welcome to your training channel", color=discord.Color.green())
            embed.description = (
                f"{student_mention}\n\n"
                f"You have recently been assigned to a training team with: {trainers_text}\n\n"
                "Please use this channel to coordinate availability and to ask questions regarding your training."
            )

            # Send trainer mentions as message content so they receive notifications; embed holds the instructions
            # Include the student mention as well so they are pinged
            content_parts = []
            if student_mention:
                content_parts.append(str(student_mention))
            content_parts.extend(trainer_mentions)
            content = " ".join(content_parts).strip()

            logger.info("Sending welcome message to channel id=%s (mentions=%s)", channel.id, trainer_mentions)
            await channel.send(content=content, embed=embed)
            logger.info("Welcome message sent to channel id=%s", channel.id)
        except Exception:
            logger.exception("Failed to send welcome message to channel id=%s", getattr(channel, "id", "<unknown>"))
            # Best-effort only; don't fail the API if the welcome message can't be sent
            pass

        return {"status": "created", "channel_id": channel.id, "guild_id": target_guild.id}

    try:
        run_op = getattr(app, "run_discord_op", None)
        if run_op is None:
            logger.error("API helper 'run_discord_op' not available on app; is the bot running?")
            raise RuntimeError("API helper 'run_discord_op' not available on app; is the bot running?")
        result = run_op(_op())
        logger.info("create_training_channel operation completed with result: %s", result)
        return jsonify(result), 200
    except Exception as e:
        logger.exception("Failed to create training channel: %s", e)
        return jsonify({"error": "Failed to create training channel", "detail": str(e)}), 500

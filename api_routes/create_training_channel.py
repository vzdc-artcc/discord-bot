from flask import Blueprint, request, jsonify
from extensions.api_server import app, api_key_required
import discord
import re
import config as cfg

bp = Blueprint("create_training_channel", __name__, url_prefix="/create_training_channel")


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
    if not data:
        return jsonify({"error": "Invalid or missing JSON body"}), 400

    student = data.get("student")
    primary = data.get("primaryTrainer")
    others = data.get("otherTrainers") or []

    # Basic validation
    if not student or not primary:
        return jsonify({"error": "Missing required student or primaryTrainer object"}), 400

    try:
        student_uid = int(student.get("discordUid"))
    except Exception:
        return jsonify({"error": "student.discordUid is required and must be an integer"}), 400

    try:
        primary_uid = int(primary.get("discordUid"))
    except Exception:
        primary_uid = None

    other_uids = []
    for o in others:
        try:
            other_uids.append(int(o.get("discordUid")))
        except Exception:
            # skip entries without valid discordUid
            continue

    first = student.get("firstName") or ""
    last = student.get("lastName") or ""
    cid = student.get("cid") or ""

    if not (first and last and cid):
        return jsonify({"error": "student.firstName, student.lastName and student.cid are required"}), 400

    raw_name = f"{first}-{last}-{cid}"
    channel_name = _slugify(raw_name)

    async def _op():
        bot = getattr(app, "bot", None)
        if bot is None:
            raise RuntimeError("Discord bot instance not available on Flask app")

        # Find guild where the student is present
        target_guild = None
        student_member = None
        for g in bot.guilds:
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
                break

        if target_guild is None:
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
        other_members = []
        for uid in other_uids:
            m = await _resolve_member(target_guild, uid)
            if m:
                other_members.append(m)


        # Determine category id from guild config (custom key 'categories.training_channels_category_id')
        guild_cfg = cfg.get_guild_config(target_guild.id)
        categories = guild_cfg.as_dict().get("categories") or {}
        cat_id = categories.get("training_channels_category_id")
        category_obj = None
        if cat_id:
            try:
                # Prefer resolving the category from the target guild to avoid cross-guild mismatches
                category_obj = target_guild.get_channel(int(cat_id))
                if category_obj is None:
                    # Fallback to fetching the channel globally and ensure it belongs to this guild
                    category_obj = await bot.fetch_channel(int(cat_id))
                    if getattr(category_obj, 'guild', None) != target_guild:
                        category_obj = None
                # Ensure the resolved object is actually a CategoryChannel
                if category_obj is not None and not isinstance(category_obj, discord.CategoryChannel):
                    category_obj = None
            except Exception:
                category_obj = None

        # Ensure channel doesn't already exist
        existing = discord.utils.get(target_guild.channels, name=channel_name)
        if existing is not None:
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
        channel = await target_guild.create_text_channel(channel_name, overwrites=overwrites, category=category_obj, reason="create_training_channel (API)")

        # Build and send a welcome embed that pings the student and trainers and includes their info
        try:
            # Mentions: prefer Member.mention when available, fallback to raw mention by id
            #            mentions = []
            #            student_mention = student_member.mention if student_member else f"<@{student_uid}>"
            #            mentions.append(student_mention)
            #            primary_mention = None
            #            if primary_member:
            #                primary_mention = primary_member.mention
            #            elif primary_uid is not None:
            #                primary_mention = f"<@{primary_uid}>"
            #            if primary_mention:
            #                mentions.append(primary_mention)
            #
            #            other_mentions = []
            #            for o_uid, o in zip(other_uids, others):
            #                # try to find resolved member in other_members matching uid
            #                m_obj = None
            #                for m in other_members:
            #                    if getattr(m, 'id', None) == int(o_uid):
            #                        m_obj = m
            #                        break
            #                other_mentions.append(m_obj.mention if m_obj else f"<@{o_uid}>")
            #            mentions.extend(other_mentions)
            #
            #            mentions_text = " ".join(mentions)
            #
            #            # Build embed with person info
            #            embed = discord.Embed(title=f"Training channel created: {channel_name}", color=discord.Color.green())
            #            embed.description = f"Welcome! This channel is for the student's training. {mentions_text}"
            #
            #            # Helper to format a person dict
            #            def fmt_person(prefix: str, person: dict):
            #                parts = []
            #                if person is None:
            #                    return "(no data)"
            #                name = person.get("fullName") or f"{person.get('firstName','')} {person.get('lastName','')}".strip()
            #                parts.append(f"Name: {name}")
            #                if person.get("operatingInitials"):
            #                    parts.append(f"Initials: {person.get('operatingInitials')}")
            #                if person.get("cid"):
            #                    parts.append(f"CID: {person.get('cid')}")
            #                if person.get("email"):
            #                    parts.append(f"Email: {person.get('email')}")
            #                if person.get("rating") is not None:
            #                    parts.append(f"Rating: {person.get('rating')}")
            #                if person.get("division"):
            #                    parts.append(f"Division: {person.get('division')}")
            #                if person.get("staffPositions"):
            #                    sp = person.get("staffPositions")
            #                    if isinstance(sp, (list, tuple)) and sp:
            #                        parts.append(f"Staff positions: {', '.join(map(str, sp))}")
            #                # Discord id
            #                if person.get("discordUid"):
            #                    parts.append(f"Discord ID: {person.get('discordUid')}")
            #                return "\n".join(parts)
            #
            #            # Student field
            #            embed.add_field(name="Student", value=fmt_person("Student", student), inline=False)
            #            # Primary trainer
            #            embed.add_field(name="Primary Trainer", value=fmt_person("Primary", primary), inline=False)
            #
            #            # Other trainers
            #            if others:
            #                other_values = []
            #                for o in others:
            #                    other_values.append(fmt_person("Trainer", o))
            #                embed.add_field(name="Other Trainers", value="\n\n".join(other_values), inline=False)
            #
            #            # Timestamp
            #            embed.timestamp = __import__('datetime').datetime.now(__import__('datetime').timezone.utc)
            #
            #            # Send the message; if this fails don't abort channel creation
            #            await channel.send(content=mentions_text, embed=embed)
            # Build trainer mention list (primary + other trainers) and prefer resolved Member.mention
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
            await channel.send(content=content, embed=embed)
        except Exception:
            # Best-effort only; don't fail the API if the welcome message can't be sent
            pass

        return {"status": "created", "channel_id": channel.id, "guild_id": target_guild.id}

    try:
        run_op = getattr(app, "run_discord_op", None)
        if run_op is None:
            raise RuntimeError("API helper 'run_discord_op' not available on app; is the bot running?")
        result = run_op(_op())
        return jsonify(result), 200
    except Exception as e:
        return jsonify({"error": "Failed to create training channel", "detail": str(e)}), 500


from flask import Blueprint, request, jsonify
import discord
from datetime import datetime, timezone
from typing import Dict, Any

from extensions.api_server import app, api_key_required
import config as cfg
from utils.events import parse_position
from utils.vatsim import parse_vatsim_logon_time
from utils.event_log import load_log, save_log, make_event_key
from bot import logger

bp = Blueprint("event_position_posting", __name__, url_prefix="/event_position_posting")


def _safe_get(d: Dict[str, Any], key: str, default=None):
    return d.get(key, default) if isinstance(d, dict) else default

RATING_ID_TO_SHORT = {
    -1: "INA",
    0: "SUS",
    1: "OBS",
    2: "S1",
    3: "S2",
    4: "S3",
    5: "C1",
    6: "C2",
    7: "C3",
    8: "I1",
    9: "I2",
    10: "I3",
    11: "SUP",
    12: "ADM",
}


@bp.route("", methods=["POST"])  # POST /event_position_posting
@api_key_required
def post_event_position_posting():
    """Accepts a payload describing an event and a list of controllers with final positions and posts

    Expected JSON shape:
    {
        "event_name": "...",
        "event_id": "...",
        "event_description": "...",
        "event_banner_url": "https://...",
        "event_start_time": "2025-11-17T00:00:00Z",
        "event_end_time": "2025-11-17T02:00:00Z",
        "controllers": [
            {"controller_name": "Alice", "controller_rating": "C1", "controller_final_position": "GND"},
            ...
        ],
        "channel_id": 1234567890,   # optional override
        "dry_run": true            # optional
    }
    """
    data = request.get_json(silent=True)
    if not data:
        logger.warning("Invalid or missing JSON payload in /event_position_posting POST")
        return jsonify({"error": "Invalid or missing JSON payload"}), 400

    # required top-level fields
    required = ["event_name", "event_id", "event_description", "event_start_time", "event_end_time", "controllers"]
    for r in required:
        if r not in data:
            logger.warning("Missing required field in /event_position_posting POST", extra={"missing_field": r, "payload_keys": list(data.keys())})
            return jsonify({"error": f"Missing required field: {r}"}), 400

    event_name = _safe_get(data, "event_name")
    event_id = _safe_get(data, "event_id")
    event_description = _safe_get(data, "event_description")
    banner_url = _safe_get(data, "event_banner_url")
    start_time = _safe_get(data, "event_start_time")
    end_time = _safe_get(data, "event_end_time")
    controllers = _safe_get(data, "controllers")
    channel_override = _safe_get(data, "channel_id")
    dry_run = bool(_safe_get(data, "dry_run", False))

    logger.debug("Processing event_position_posting request", extra={"event_id": event_id, "event_name": event_name, "guild_id": _safe_get(data, "guild_id"), "dry_run": dry_run, "channel_override": channel_override})

    if not isinstance(controllers, list):
        logger.warning("Invalid controllers type in request; expected list", extra={"controllers_type": type(controllers).__name__})
        return jsonify({"error": "`controllers` must be a list"}), 400

    # format times for Discord timestamps if possible
    times_str = ""
    try:
        sdt = parse_vatsim_logon_time(start_time) if isinstance(start_time, str) and start_time else (start_time if isinstance(start_time, datetime) else None)
        edt = parse_vatsim_logon_time(end_time) if isinstance(end_time, str) and end_time else (end_time if isinstance(end_time, datetime) else None)
        if isinstance(sdt, datetime) and sdt.tzinfo is None:
            sdt = sdt.replace(tzinfo=timezone.utc)
        if isinstance(edt, datetime) and edt.tzinfo is None:
            edt = edt.replace(tzinfo=timezone.utc)
        if sdt and edt:
            times_str = f" — <t:{int(sdt.timestamp())}:F> to <t:{int(edt.timestamp())}:F>"
        elif sdt:
            times_str = f" — <t:{int(sdt.timestamp())}:F>"
        elif edt:
            times_str = f" — <t:{int(edt.timestamp())}:F>"
        else:
            if start_time and end_time:
                times_str = f" — {start_time} to {end_time} (UTC)"
            elif start_time:
                times_str = f" — starts {start_time} (UTC)"
        logger.debug("Parsed event times", extra={"start_time": start_time, "end_time": end_time, "times_str": times_str})
    except Exception:
        logger.debug("Failed to parse event start/end times", exc_info=True)
        times_str = ""

    # Resolve announcement type properties
    post_conf = cfg.ANNOUNCEMENT_TYPES.get("event-position-posting", {})
    color_val = post_conf.get("color")
    title_prefix = post_conf.get("title_prefix", "Event Posting:")

    # Resolve target channel via explicit override or guild config (if provided)
    guild_id = _safe_get(data, "guild_id")
    target_channel_id = None
    if channel_override:
        target_channel_id = int(channel_override)
        logger.debug("Using channel override for announcement", extra={"target_channel_id": target_channel_id})
    elif guild_id is not None:
        target_channel_id = cfg.resolve_announcement_target_channel(guild_id, "event-position-posting")
        logger.debug("Resolved target channel from guild config", extra={"guild_id": guild_id, "target_channel_id": target_channel_id})

    # Build embed
    embed_title = f"{title_prefix} {event_name}".strip()
    embed = discord.Embed(title=embed_title, description=event_description or "", color=discord.Color(color_val) if color_val is not None else discord.Color.default())

    if banner_url:
        try:
            embed.set_image(url=banner_url)
        except Exception as e:
            # ignore bad banner url
            logger.debug("Failed to set embed image", exc_info=True, extra={"banner_url": banner_url})

    # Controllers: group controllers by position category (GND/TWR/APP/CTR/etc.) and add one field per category
    max_fields = 25
    added = 0

    # Aggregate controllers into categories using parse_position
    category_groups: dict = {}
    for c in controllers:
        if not isinstance(c, dict):
            logger.debug("Skipping controller entry that is not a dict", extra={"entry": c})
            continue
        final_pos = _safe_get(c, "controller_final_position")
        if not final_pos:
            # skip controllers without a final position
            logger.debug("Skipping controller with no final position", extra={"controller": c})
            continue

        # determine category (e.g., GND, TWR, APP, CTR, etc.)
        try:
            category = parse_position(str(final_pos)) or "UNKNOWN"
        except Exception:
            logger.debug("Failed to parse controller final position; defaulting to UNKNOWN", exc_info=True, extra={"final_pos": final_pos})
            category = "UNKNOWN"

        name = _safe_get(c, "controller_name") or "(Unnamed)"
        rating = _safe_get(c, "controller_rating")
        # format rating consistently as string if present
        rating_str = ""
        if rating is not None:
            # If rating is an integer (or numeric string), map using RATING_ID_TO_SHORT
            try:
                # handle ints directly
                if isinstance(rating, int):
                    rating_str = RATING_ID_TO_SHORT.get(rating, str(rating))
                else:
                    rs = str(rating).strip()
                    # numeric string (e.g. "5")
                    if rs.lstrip("+-").isdigit():
                        try:
                            rid = int(rs)
                            rating_str = RATING_ID_TO_SHORT.get(rid, rs)
                        except Exception:
                            rating_str = rs.upper()
                    else:
                        # If the provided value is already a known short code, normalize it.
                        up = rs.upper()
                        if up in set(RATING_ID_TO_SHORT.values()):
                            rating_str = up
                        else:
                            # Fallback: use the string representation (uppercased for consistency)
                            rating_str = up
            except Exception:
                logger.debug("Failed to normalize controller rating", exc_info=True, extra={"rating": rating})
                rating_str = str(rating)

        # readable line for this controller
        if rating_str:
            line = f"{name} ({rating_str}) — {final_pos}"
        else:
            line = f"{name} — {final_pos}"

        category_groups.setdefault(category, []).append(line)

    logger.debug("Grouped controllers into categories", extra={"category_counts": {k: len(v) for k, v in category_groups.items()}})

    # Preferred display order for categories, fall back to remaining sorted keys
    preferred_order = ["RMP", "DEL", "GND", "TWR", "DEP", "APP", "CTR", "CIC", "TMU", "OTHER", "UNKNOWN"]
    remaining = [k for k in category_groups.keys() if k not in preferred_order]
    ordered_keys = [k for k in preferred_order if k in category_groups] + sorted(remaining)

    # Add one embed field per category with all controllers in that category (truncate if too long)
    for cat in ordered_keys:
        if added >= max_fields:
            logger.info("Reached max embed fields limit; skipping remaining categories", extra={"added": added, "max_fields": max_fields})
            break
        members = category_groups.get(cat, [])
        if not members:
            continue

        field_name = f"{cat} ({len(members)})"
        # join members with newlines; Discord field value max ~1024 characters
        value = "\n".join(members)
        if len(value) > 1000:
            # try to truncate gracefully while keeping whole lines
            parts = []
            cur_len = 0
            for m in members:
                if cur_len + len(m) + 1 > 996:
                    parts.append("...")
                    break
                parts.append(m)
                cur_len += len(m) + 1
            value = "\n".join(parts)
            logger.info("Truncated embed field to fit Discord limits", extra={"field": field_name, "truncated_len": len(value)})

        embed.add_field(name=field_name, value=value, inline=False)
        added += 1

    # If no category fields were added, add a summary field
    if added == 0:
        logger.info("No controllers provided or none valid; adding placeholder field to embed", extra={"event_id": event_id})
        embed.add_field(name="Controllers", value="No controllers provided or none valid.", inline=False)

    # Append times to title or description for visibility
    if times_str:
        # tack onto title for compact view
        embed.title = f"{embed.title}{times_str}"
        logger.debug("Appended formatted times to embed title", extra={"times_str": times_str})

    # Dry-run: return the prepared payload
    if dry_run:
        payload = {
            "title": embed.title,
            "description": embed.description,
            "color": color_val,
            "image_url": banner_url,
            "fields": [{"name": f.name, "value": f.value} for f in embed.fields],
            "target_channel_id": target_channel_id,
        }
        logger.info("Dry-run: prepared event posting payload", extra={"event_id": event_id, "target_channel_id": target_channel_id})
        logger.debug("Dry-run payload", extra={"payload": payload})
        return jsonify({"status": "dry_run", "payload": payload}), 200

    # If we don't have a target channel by this point, fail early with a helpful message
    if target_channel_id is None:
        logger.error("No target channel resolved for event posting; aborting", extra={"event_id": event_id, "guild_id": guild_id})
        return jsonify({"error": "No target channel configured or provided for event posting"}), 400

    # Send to Discord via app.run_discord_op
    async def _send():
        bot = getattr(app, "bot", None)
        if bot is None:
            logger.error("Discord bot instance not available on Flask app during _send")
            raise RuntimeError("Discord bot instance not available on Flask app")

        # target_channel_id must be present here; guard against accidental None to avoid TypeError from int(None)
        if target_channel_id is None:
            logger.error("Attempted to send without a target_channel_id", extra={"event_id": event_id})
            raise RuntimeError("No target channel specified for posting")

        channel = bot.get_channel(int(target_channel_id)) if target_channel_id is not None else None
        if channel is None:
            try:
                channel = await bot.fetch_channel(int(target_channel_id))
            except Exception as e:
                logger.exception("Failed to fetch channel inside _send", exc_info=True, extra={"target_channel_id": target_channel_id})
                raise RuntimeError(f"Failed to fetch channel {target_channel_id}: {e}")

        sent = await channel.send(embed=embed)
        return getattr(sent, "id", None)

    try:
        run_op = getattr(app, "run_discord_op", None)
        if run_op is None:
            logger.error("API server helper 'run_discord_op' not available on app; is the bot running?")
            raise RuntimeError("API server helper 'run_discord_op' not available on app; is the bot running?")
        logger.info("Posting embed to Discord", extra={"event_id": event_id, "target_channel_id": target_channel_id})
        message_id = run_op(_send())
        logger.info("Posted event embed to Discord", extra={"event_id": event_id, "message_id": message_id, "target_channel_id": target_channel_id})

        # After successful post, persist the posting and delete any previous posting for same event
        try:
            key = make_event_key(event_id, event_name, guild_id)
            guild_key = guild_id if guild_id is not None else None
            log = load_log(guild_key) or {}

            existing = log.get(key)
            if existing:
                prev_chan = existing.get("channel_id")
                prev_msg = existing.get("message_id")
                if prev_chan and prev_msg:
                    logger.info("Found existing posting for event; attempting to delete previous message", extra={"prev_channel": prev_chan, "prev_msg": prev_msg})
                    # Try to delete previous message asynchronously via run_op
                    async def _delete_prev():
                        try:
                            bot = getattr(app, "bot", None)
                            if bot is None:
                                logger.debug("Bot missing when attempting to delete previous message")
                                return False
                            try:
                                channel = bot.get_channel(int(prev_chan)) if prev_chan is not None else None
                                if channel is None:
                                    channel = await bot.fetch_channel(int(prev_chan))
                                # fetch message and delete
                                try:
                                    msg = await channel.fetch_message(int(prev_msg))
                                except Exception:
                                    # message may be already deleted or fetch failed
                                    logger.debug("Failed to fetch previous message; it may already be deleted", extra={"prev_channel": prev_chan, "prev_msg": prev_msg})
                                    msg = None
                                if msg:
                                    await msg.delete()
                                    logger.info("Deleted previous event posting message", extra={"prev_channel": prev_chan, "prev_msg": prev_msg})
                                return True
                            except Exception:
                                logger.debug("Error while trying to delete previous message", exc_info=True, extra={"prev_channel": prev_chan, "prev_msg": prev_msg})
                                return False
                        except Exception:
                            logger.debug("Unexpected error in _delete_prev", exc_info=True)
                            return False

                    try:
                        # fire deletion; ignore result but attempt it
                        run_op(_delete_prev())
                    except Exception:
                        logger.debug("run_op failed when attempting to delete previous message", exc_info=True)
                        # ignore deletion failure; proceed to update log
                        pass

            # record new entry
            entry = {
                "event_title": event_name,
                "event_id": event_id,
                "guild_id": guild_id,
                "channel_id": target_channel_id,
                "message_id": message_id,
                "timestamp": datetime.now(timezone.utc).isoformat(),
            }
            log[key] = entry
            try:
                save_log(guild_key, log)
                logger.info("Persisted posting to event log", extra={"key": key, "guild_key": guild_key})
            except Exception:
                logger.warning("Failed to persist posting log", exc_info=True, extra={"key": key, "guild_key": guild_key})
                # If saving the log fails, we still succeeded in posting; surface error to caller
                return jsonify({"status": "ok", "channel_id": target_channel_id, "message_id": message_id, "warning": "failed to persist posting log"}), 200
        except Exception:
            logger.debug("Error while handling posting log; continuing (non-fatal)", exc_info=True)
            # Any error while handling logs should not break the main success path
            return jsonify({"status": "ok", "channel_id": target_channel_id, "message_id": message_id}), 200

        return jsonify({"status": "ok", "channel_id": target_channel_id, "message_id": message_id}), 200
    except Exception as e:
        logger.exception("Failed to post event positions", exc_info=True, extra={"event_id": event_id, "target_channel_id": target_channel_id})
        return jsonify({"error": "Failed to post event positions", "detail": str(e)}), 500


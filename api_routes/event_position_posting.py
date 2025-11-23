from flask import Blueprint, request, jsonify
import discord
from datetime import datetime, timezone
from typing import Dict, Any

from extensions.api_server import app, api_key_required
from config import ANNOUNCEMENT_TYPES
from utils.events import parse_position
from utils.vatsim import parse_vatsim_logon_time

bp = Blueprint("event_position_posting", __name__, url_prefix="/event_position_posting")


def _safe_get(d: Dict[str, Any], key: str, default=None):
    return d.get(key, default) if isinstance(d, dict) else default


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
        return jsonify({"error": "Invalid or missing JSON payload"}), 400

    # required top-level fields
    required = ["event_name", "event_id", "event_description", "event_start_time", "event_end_time", "controllers"]
    for r in required:
        if r not in data:
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

    if not isinstance(controllers, list):
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
    except Exception:
        times_str = ""

    # Resolve announcement type properties
    # Use the existing announcement config for event postings
    post_conf = ANNOUNCEMENT_TYPES.get("event-position-posting", {})
    color_val = post_conf.get("color")
    title_prefix = post_conf.get("title_prefix", "Event Posting:")
    target_channel_id = channel_override or post_conf.get("channel_id")

    # Build embed
    embed_title = f"{title_prefix} {event_name}".strip()
    embed = discord.Embed(title=embed_title, description=event_description or "", color=discord.Color(color_val) if color_val is not None else discord.Color.default())

    if banner_url:
        try:
            embed.set_image(url=banner_url)
        except Exception:
            # ignore bad banner url
            pass

    # Controllers: group controllers by position category (GND/TWR/APP/CTR/etc.) and add one field per category
    max_fields = 25
    added = 0

    # Aggregate controllers into categories using parse_position
    category_groups: dict = {}
    for c in controllers:
        if not isinstance(c, dict):
            continue
        final_pos = _safe_get(c, "controller_final_position")
        if not final_pos:
            # skip controllers without a final position
            continue

        # determine category (e.g., GND, TWR, APP, CTR, etc.)
        try:
            category = parse_position(str(final_pos)) or "UNKNOWN"
        except Exception:
            category = "UNKNOWN"

        name = _safe_get(c, "controller_name") or "(Unnamed)"
        rating = _safe_get(c, "controller_rating")
        # format rating consistently as string if present
        rating_str = f"{rating}" if rating is not None else ""

        # readable line for this controller
        if rating_str:
            line = f"{name} ({rating_str}) — {final_pos}"
        else:
            line = f"{name} — {final_pos}"

        category_groups.setdefault(category, []).append(line)

    # Preferred display order for categories, fall back to remaining sorted keys
    preferred_order = ["RMP", "DEL", "GND", "TWR", "APP", "CTR", "DEP", "OTHER", "UNKNOWN"]
    remaining = [k for k in category_groups.keys() if k not in preferred_order]
    ordered_keys = [k for k in preferred_order if k in category_groups] + sorted(remaining)

    # Add one embed field per category with all controllers in that category (truncate if too long)
    for cat in ordered_keys:
        if added >= max_fields:
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

        embed.add_field(name=field_name, value=value, inline=False)
        added += 1

    # If no category fields were added, add a summary field
    if added == 0:
        embed.add_field(name="Controllers", value="No controllers provided or none valid.", inline=False)

    # Append times to title or description for visibility
    if times_str:
        # tack onto title for compact view
        embed.title = f"{embed.title}{times_str}"

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
        return jsonify({"status": "dry_run", "payload": payload}), 200

    # Send to Discord via app.run_discord_op
    async def _send():
        bot = getattr(app, "bot", None)
        if bot is None:
            raise RuntimeError("Discord bot instance not available on Flask app")

        channel = bot.get_channel(int(target_channel_id)) if target_channel_id is not None else None
        if channel is None:
            try:
                channel = await bot.fetch_channel(int(target_channel_id))
            except Exception as e:
                raise RuntimeError(f"Failed to fetch channel {target_channel_id}: {e}")

        sent = await channel.send(embed=embed)
        return getattr(sent, "id", None)

    try:
        run_op = getattr(app, "run_discord_op", None)
        if run_op is None:
            raise RuntimeError("API server helper 'run_discord_op' not available on app; is the bot running?")
        message_id = run_op(_send())
        return jsonify({"status": "ok", "channel_id": target_channel_id, "message_id": message_id}), 200
    except Exception as e:
        return jsonify({"error": "Failed to post event positions", "detail": str(e)}), 500

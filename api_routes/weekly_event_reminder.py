from flask import Blueprint, request, jsonify
import discord
from typing import Dict, Any
import os
from datetime import datetime, timezone
from utils.vatsim import parse_vatsim_logon_time
from extensions.api_server import app, api_key_required
import config as cfg

bp = Blueprint("weekly_event_reminder", __name__, url_prefix="/weekly_event_reminder")


def _safe_get(d: Dict[str, Any], key: str, default=None):
    return d.get(key, default) if isinstance(d, dict) else default


@bp.route("", methods=["POST"])  # POST /regular_event_reminder
@api_key_required
def post_weekly_event_reminder():
    """Accepts a payload with a list of events for the week and posts an embed for each

    Expected JSON shape includes either `guild_id` or `channel_id` to determine where the message should be posted.
    """
    data = request.get_json(silent=True)
    if not data:
        return jsonify({"error": "Invalid or missing JSON payload"}), 400

    events = data.get("events")
    if not isinstance(events, list) or len(events) == 0:
        return jsonify({"error": "`events` must be a non-empty list"}), 400

    guild_id = data.get("guild_id")
    channel_override = data.get("channel_id")

    # Resolve the target channel id
    target_channel_id = None
    if channel_override:
        target_channel_id = int(channel_override)
    elif guild_id is not None:
        target_channel_id = cfg.resolve_announcement_target_channel(guild_id, "event-reminder")

    if target_channel_id is None:
        return jsonify({"error": "No target channel determined. Provide `guild_id` or `channel_id`."}), 400

    # Store events in the Flask app's event_store so button interaction handlers can access details.
    try:
        if not hasattr(app, "event_store") or not isinstance(getattr(app, "event_store"), dict):
            app.event_store = {}
    except Exception:
        app.event_store = {}

    # TTL for stored events in seconds (default 24 hours). Can be overridden with EVENT_STORE_TTL env var.
    EVENT_STORE_TTL = int(os.getenv("EVENT_STORE_TTL", "86400"))

    now_ts = datetime.now(timezone.utc).timestamp()
    for ev in events:
        ev_id = _safe_get(ev, "event_id")
        if ev_id:
            try:
                key = str(ev_id).strip()
                app.event_store[key] = {
                    "event": ev,
                    "expires_at": now_ts + EVENT_STORE_TTL,
                }
            except Exception:
                # ignore failures to store event (non-critical)
                pass

    # Build one or more embeds (max 25 fields per embed). For each embed chunk we'll create a montage
    # image out of that chunk's banners and attach it to the message. This preserves the "single embed"
    # feeling per chunk while still supporting many events.
    from math import ceil
    from io import BytesIO
    import urllib.request
    try:
        from PIL import Image, ImageDraw, ImageFont
    except Exception:
        Image = ImageDraw = ImageFont = None

    # Local copies for nested helpers and static analysis
    prefix = cfg.ANNOUNCEMENT_TYPES.get("event-reminder", {}).get("title_prefix", "Event Reminder:")
    color = cfg.ANNOUNCEMENT_TYPES.get("event-reminder", {}).get("color", 0x2F3136)
    max_field_name = 256
    max_field_value = 1024

    def _make_field_for_event(ev: Dict[str, Any]):
        name = _safe_get(ev, "event_name") or "(Unnamed event)"
        event_id = _safe_get(ev, "event_id") or ""
        desc = _safe_get(ev, "event_description") or "(No description)"
        start = _safe_get(ev, "event_start_time") or ""
        end = _safe_get(ev, "event_end_time") or ""
        ev_type = _safe_get(ev, "event_type") or ""
        host = _safe_get(ev, "event_host") or ""
        featured = _safe_get(ev, "event_feature_fields") or []

        # times
        times = ""
        try:
            sdt = parse_vatsim_logon_time(start) if isinstance(start, str) and start else (start if isinstance(start, datetime) else None)
            edt = parse_vatsim_logon_time(end) if isinstance(end, str) and end else (end if isinstance(end, datetime) else None)
            if isinstance(sdt, datetime) and sdt.tzinfo is None:
                sdt = sdt.replace(tzinfo=timezone.utc)
            if isinstance(edt, datetime) and edt.tzinfo is None:
                edt = edt.replace(tzinfo=timezone.utc)
            if sdt and edt:
                times = f" — <t:{int(sdt.timestamp())}:F> to <t:{int(edt.timestamp())}:F>"
            elif sdt:
                times = f" — <t:{int(sdt.timestamp())}:F>"
            elif edt:
                times = f" — <t:{int(edt.timestamp())}:F>"
            else:
                if start and end:
                    times = f" — {start} to {end} (UTC)"
                elif start:
                    times = f" — starts {start} (UTC)"
        except Exception:
            times = ""

        field_name = f"{name}{times}"

        parts = [desc]
        meta = []
        if ev_type:
            meta.append(f"Type: {ev_type}")
        if host:
            meta.append(f"Host: {host}")
        if meta:
            parts.append(" • ".join(meta))
        if isinstance(featured, list) and all(isinstance(x, str) for x in featured) and featured:
            parts.append("Featured: " + ", ".join(featured))
        if event_id:
            parts.append(f"[Event Page](https://vzdc.org/events/{str(event_id).strip()})")

        value = "\n".join(parts)
        if len(value) > (max_field_value - 4):
            value = value[: (max_field_value - 7)] + "..."

        return field_name, value

    # chunk events and prepare embeds + list of banner URLs per chunk
    chunk_size = 25
    chunks = [events[i:i+chunk_size] for i in range(0, len(events), chunk_size)]
    embeds = []

    # Montage settings
    thumb_w = 320
    thumb_h = 180
    cols = 3

    for chunk in chunks:
        embed = discord.Embed(title=f"{prefix} Weekly Events", color=color)
        embed.description = f"{len(chunk)} event(s) this message."

        banner_urls = []
        for ev in chunk:
            fname, fval = _make_field_for_event(ev)
            if len(fname) > max_field_name:
                fname = fname[: (max_field_name - 4)] + "..."
            embed.add_field(name=fname, value=fval, inline=False)
            b = _safe_get(ev, "event_banner_url")
            if b:
                banner_urls.append(b)

        # Download banners synchronously and compose a montage image
        imgs = []
        for url in banner_urls:
            try:
                with urllib.request.urlopen(url, timeout=10) as resp:
                    data = resp.read()
                img = Image.open(BytesIO(data)).convert("RGB")
                imgs.append(img)
            except Exception:
                continue

        montage_bytes = None
        if imgs:
            # compute grid size
            count = len(imgs)
            cols_use = min(cols, count)
            rows = ceil(count / cols_use)
            canvas_w = cols_use * thumb_w
            canvas_h = rows * thumb_h
            montage = Image.new("RGB", (canvas_w, canvas_h), (40, 40, 40))

            for idx, im in enumerate(imgs):
                # resize/crop to thumbnail area while preserving aspect
                im_thumb = im.copy()
                im_thumb.thumbnail((thumb_w, thumb_h), Image.LANCZOS)
                # paste centered in slot
                row = idx // cols_use
                col = idx % cols_use
                x = col * thumb_w + (thumb_w - im_thumb.width) // 2
                y = row * thumb_h + (thumb_h - im_thumb.height) // 2
                montage.paste(im_thumb, (x, y))

            bio = BytesIO()
            montage.save(bio, format="PNG")
            bio.seek(0)
            montage_bytes = bio.read()

        # If no banners were downloaded to create a montage, produce a small placeholder
        if montage_bytes is None and Image is not None:
            try:
                # single placeholder image sized for one thumbnail slot
                ph_w = thumb_w * min(cols, 1)
                ph_h = thumb_h
                placeholder = Image.new("RGB", (ph_w, ph_h), (40, 40, 40))
                draw = ImageDraw.Draw(placeholder)
                msg = "No banners available"
                try:
                    # Use a default font if available
                    font = ImageFont.load_default() if ImageFont is not None else None
                    text_w, text_h = draw.textsize(msg, font=font)
                    draw.text(((ph_w - text_w) / 2, (ph_h - text_h) / 2), msg, fill=(200, 200, 200), font=font)
                except Exception:
                    try:
                        draw.text((10, 10), msg, fill=(200, 200, 200))
                    except Exception:
                        pass
                bio = BytesIO()
                placeholder.save(bio, format="PNG")
                bio.seek(0)
                montage_bytes = bio.read()
            except Exception:
                montage_bytes = None

        # If still no montage_bytes, fall back to a tiny embedded PNG so the embed has an image.
        if montage_bytes is None:
            try:
                import base64
                # 1x1 transparent PNG (very small). This guarantees an image attachment even without Pillow.
                placeholder_png_b64 = (
                    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNgYAAAAAMAASsJTYQAAAAA"
                    "SUVORK5CYII="
                )
                montage_bytes = base64.b64decode(placeholder_png_b64)
            except Exception:
                montage_bytes = None

        # Build up to 5 link buttons for this chunk (first 5 events with an id)
        view = None
        try:
            buttons = []
            for ev in chunk[:5]:
                ev_id = _safe_get(ev, "event_id") or ""
                if not ev_id:
                    continue
                label = _safe_get(ev, "event_name") or ev_id
                if len(label) > 80:
                    label = label[:77] + "..."
                url = f"https://vzdc.org/events/{str(ev_id).strip()}"
                btn = discord.ui.Button(style=discord.ButtonStyle.link, label=label, url=url)  # type: ignore
                buttons.append(btn)
            if buttons:
                view = discord.ui.View()
                for b in buttons:
                    view.add_item(b)
        except Exception:
            view = None

        embeds.append((embed, view, montage_bytes))
        embed.timestamp = datetime.now(timezone.utc)
        embed.set_footer(text="vZDC", icon_url=guild_id.icon.url if guild_id.icon else None)

    # Send all embeds sequentially to the configured channel using Flask app helper

    async def _send_all():
        bot = getattr(app, "bot", None)
        if bot is None:
            raise RuntimeError("Discord bot not attached to Flask app")

        # Try get_channel (cache) then fetch_channel
        channel = bot.get_channel(target_channel_id)
        if channel is None:
            try:
                channel = await bot.fetch_channel(target_channel_id)
            except Exception:
                channel = None

        if channel is None:
            raise RuntimeError(f"Could not find channel with id {target_channel_id}")

        sent_ids = []
        for emb in embeds:
            # each embed entry is a tuple (embed, view, montage_bytes)
            embed_obj, view_obj, montage_bytes = emb
            file_obj = None
            if montage_bytes:
                try:
                    file_obj = discord.File(BytesIO(montage_bytes), filename="montage.png")
                    # set embed image to attachment
                    embed_obj.set_image(url="attachment://montage.png")
                except Exception:
                    file_obj = None

            try:
                if file_obj is not None:
                    msg = await channel.send(embed=embed_obj, view=view_obj, file=file_obj)
                else:
                    msg = await channel.send(embed=embed_obj, view=view_obj)
            except TypeError:
                # Older discord.py versions or incompatible send signature may not accept view; fallback
                if file_obj is not None:
                    msg = await channel.send(embed=embed_obj, file=file_obj)
                else:
                    msg = await channel.send(embed=embed_obj)
            sent_ids.append(getattr(msg, "id", None))
        return sent_ids

    try:
        # use getattr to avoid static-analysis warnings about dynamic attributes
        run_op = getattr(app, "run_discord_op", None)
        if run_op is None:
            raise RuntimeError("Flask app missing run_discord_op helper")
        result = run_op(_send_all())
    except Exception as exc:
        return jsonify({"error": "Failed to deliver embeds to Discord", "detail": str(exc)}), 500

    # (No interactive custom-button handler is registered here; each embed includes a link button
    # and the embed title is clickable because embed.url is set to the event page when available.)

    return jsonify({"status": "ok", "sent": len(embeds), "message_ids": result}), 200

from flask import Blueprint, request, jsonify
import discord
from typing import Dict, Any, Tuple
import os
from datetime import datetime, timezone
from utils.vatsim import parse_vatsim_logon_time
from extensions.api_server import app, api_key_required
from config import ANNOUNCEMENT_TYPES

bp = Blueprint("regular_event_reminder", __name__, url_prefix="/regular_event_reminder")


def _safe_get(d: Dict[str, Any], key: str, default=None):
    return d.get(key, default) if isinstance(d, dict) else default


@bp.route("", methods=["POST"])  # POST /regular_event_reminder
@api_key_required
def post_regular_event_reminder():
    """Accepts a payload with a list of events for the week and posts an embed for each

    Expected JSON shape:
    {
        "events": [
            {
                "event_name": "...",
                "event_id": "...",
                "event_description": "...",
                "event_start_time": "2025-11-17T00:00:00Z",
                "event_end_time": "2025-11-17T02:00:00Z",
                "event_banner_url": "https://...",
                "event_type": "training",
                "event_host": "Example Host",
                "event_featured_fields": [ {"name":"Field","value":"Value"}, ... ]
            },
            ...
        ]
    }
    """
    data = request.get_json(silent=True)
    if not data:
        return jsonify({"error": "Invalid or missing JSON payload"}), 400

    events = data.get("events")
    if not isinstance(events, list) or len(events) == 0:
        return jsonify({"error": "`events` must be a non-empty list"}), 400

    # Store events in the Flask app's event_store so button interaction handlers can access details.
    # Initialize event_store if missing.
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

    # Build one or more discord.Embed objects that include events as separate fields.
    prefix = ANNOUNCEMENT_TYPES.get("event-reminder", {}).get("title_prefix", "Event Reminder:")
    color = ANNOUNCEMENT_TYPES.get("event-reminder", {}).get("color", 0x2F3136)

    # Discord limits: max 25 fields per embed, field name <= 256 chars, field value <= 1024 chars
    max_fields_per_embed = 25
    max_field_name = 256
    max_field_value = 1024

    # We'll chunk events into multiple embeds each containing up to `max_fields_per_embed` events.
    embeds = []

    def _make_event_field(ev: Dict[str, Any]) -> Tuple[str, str]:
        name = _safe_get(ev, "event_name") or "(Unnamed event)"
        event_id = _safe_get(ev, "event_id") or ""
        desc = _safe_get(ev, "event_description") or "(No description)"
        start = _safe_get(ev, "event_start_time") or ""
        end = _safe_get(ev, "event_end_time") or ""
        banner = _safe_get(ev, "event_banner_url") or None
        ev_type = _safe_get(ev, "event_type") or ""
        host = _safe_get(ev, "event_host") or ""
        featured = _safe_get(ev, "event_feature_fields") or []

        # Field name: Event name plus times if available
        times = ""
        # Try to parse start/end into datetimes and format as Discord timestamp tags like <t:...:F>
        start_dt = None
        end_dt = None
        try:
            if isinstance(start, str) and start:
                start_dt = parse_vatsim_logon_time(start)
            elif isinstance(start, datetime):
                start_dt = start
            if isinstance(end, str) and end:
                end_dt = parse_vatsim_logon_time(end)
            elif isinstance(end, datetime):
                end_dt = end
            # Ensure timezone-aware in UTC for timestamp calculation
            if start_dt and start_dt.tzinfo is None:
                start_dt = start_dt.replace(tzinfo=timezone.utc)
            if end_dt and end_dt.tzinfo is None:
                end_dt = end_dt.replace(tzinfo=timezone.utc)
        except Exception:
            # parsing failed; fall back to raw strings
            start_dt = None
            end_dt = None

        if start_dt and end_dt:
            times = f" — <t:{int(start_dt.timestamp())}:F> to <t:{int(end_dt.timestamp())}:F>"
        elif start_dt:
            times = f" — <t:{int(start_dt.timestamp())}:F>"
        elif end_dt:
            times = f" — <t:{int(end_dt.timestamp())}:F>"
        else:
            # fallback to raw ISO strings if parsing failed
            if start and end:
                times = f" — {start} to {end} (UTC)"
            elif start:
                times = f" — starts {start} (UTC)"
            elif end:
                times = f" — ends {end} (UTC)"

        field_name = f"{name}{times}"

        # Field value: description + type/host + featured fields + id
        parts = [desc]
        meta = []
        if ev_type:
            meta.append(f"Type: {ev_type}")
        if host:
            meta.append(f"Host: {host}")
        if meta:
            parts.append(" • ".join(meta))

        # Featured fields: expect list of strings
        if isinstance(featured, list) and all(isinstance(x, str) for x in featured):
            if featured:
                parts.append("Featured: " + ", ".join(featured))
        else:
            # If it's not a list of strings, ignore it to enforce the new schema
            pass

        if event_id:
            try:
                event_id_safe = str(event_id).strip()
                if event_id_safe:
                    event_url = f"https://vzdc.org/events/{event_id_safe}"
                    parts.append(f"[Event Page]({event_url})")
            except Exception:
                # If anything goes wrong creating the URL, silently skip the link
                pass

        # Join parts and ensure field value is within Discord limits (1024 chars)
        value = "\n".join(parts)
        if len(value) > (max_field_value - 4):
            value = value[: (max_field_value - 7)] + "..."

        return field_name, value

    # Chunk events into embeds
    total_chunks = (len(events) + max_fields_per_embed - 1) // max_fields_per_embed
    for chunk_idx in range(total_chunks):
        chunk_events = events[chunk_idx * max_fields_per_embed: (chunk_idx + 1) * max_fields_per_embed]
        embed_title = f"{prefix} Weekly Events"
        if total_chunks > 1:
            embed_title = f"{embed_title} ({chunk_idx+1}/{total_chunks})"
        e = discord.Embed(title=embed_title, color=color)
        e.description = f"Showing {len(chunk_events)} event(s) — total {len(events)} this week."

        # Attach the first available banner URL from this chunk as the embed image (if present)
        banner_url = None
        for ev in chunk_events:
            b = _safe_get(ev, "event_banner_url")
            if b:
                banner_url = b
                break
        if banner_url:
            try:
                e.set_image(url=banner_url)
            except Exception:
                # if Discord rejects the URL or something goes wrong, continue without the image
                banner_url = None

        for ev in chunk_events:
            fname, fval = _make_event_field(ev)
            # truncate field name to Discord limits
            if len(fname) > max_field_name:
                fname = fname[: (max_field_name - 4)] + "..."
            e.add_field(name=fname, value=fval, inline=False)

        # Build up to 5 link buttons for the first events in this embed chunk
        view = None
        try:
            buttons = []
            for ev in chunk_events[:5]:
                ev_id = _safe_get(ev, "event_id") or ""
                if not ev_id:
                    continue
                label = _safe_get(ev, "event_name") or ev_id
                # Discord button label limit is 80 characters
                if len(label) > 80:
                    label = label[:77] + "..."
                # Use a non-link button with a custom_id so the bot can handle the interaction
                # custom_id limit is 100 chars; use prefix 'evt:'
                custom_id = f"evt:{str(ev_id).strip()}"
                # Use primary style for a call-to-action button
                btn = discord.ui.Button(style=discord.ButtonStyle.primary, label=label, custom_id=custom_id)  # type: ignore
                buttons.append(btn)

            if buttons:
                view = discord.ui.View()
                for b in buttons:
                    view.add_item(b)
        except Exception:
            view = None

        embeds.append((e, view))

    # Send all embeds sequentially to the configured channel using Flask app helper
    channel_id = ANNOUNCEMENT_TYPES.get("event-reminder", {}).get("channel_id")
    if not channel_id:
        return jsonify({"error": "Event announcement channel not configured on server"}), 500

    async def _send_all():
        bot = getattr(app, "bot", None)
        if bot is None:
            raise RuntimeError("Discord bot not attached to Flask app")

        # Try get_channel (cache) then fetch_channel
        channel = bot.get_channel(channel_id)
        if channel is None:
            try:
                channel = await bot.fetch_channel(channel_id)
            except Exception:
                channel = None

        if channel is None:
            raise RuntimeError(f"Could not find channel with id {channel_id}")

        sent_ids = []
        for emb in embeds:
            # each embed entry is a tuple (embed, view)
            if isinstance(emb, tuple) and len(emb) == 2:
                embed_obj, view_obj = emb
            else:
                embed_obj, view_obj = emb, None
            try:
                msg = await channel.send(embed=embed_obj, view=view_obj)
            except TypeError:
                # Older discord.py versions or incompatible send signature may not accept view; fallback
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

    # Ensure the bot has an interaction handler for our event buttons registered.
    # We'll register a single listener on the bot to handle custom_id values like 'evt:<event_id>'.
    def _register_interaction_handler():
        bot = getattr(app, "bot", None)
        if bot is None:
            return

        # Avoid registering the handler multiple times
        if getattr(app, "_evt_listener_registered", False):
            return

        async def _interaction_handler(interaction: discord.Interaction):
            try:
                if interaction.type != discord.InteractionType.component:
                    return
                data = getattr(interaction, "data", None) or {}
                custom_id = data.get("custom_id")
                if not custom_id or not str(custom_id).startswith("evt:"):
                    return

                event_id = str(custom_id).split(":", 1)[1]
                entry = getattr(app, "event_store", {}).get(event_id)
                ev = None
                if entry:
                    try:
                        expires = entry.get("expires_at")
                        now = datetime.now(timezone.utc).timestamp()
                        if expires is None or expires < now:
                            # expired: remove and treat as missing
                            try:
                                del app.event_store[event_id]
                            except Exception:
                                pass
                            ev = None
                        else:
                            ev = entry.get("event")
                    except Exception:
                        ev = entry.get("event") if isinstance(entry, dict) else None
                if not ev:
                     resp = getattr(interaction, "response", None)
                     if resp and hasattr(resp, "send_message"):
                         await resp.send_message("Event not found or expired.", ephemeral=True)
                     else:
                         try:
                             await interaction.followup.send("Event not found or expired.", ephemeral=True)
                         except Exception:
                             pass
                     return

                # Build and send an ephemeral embed with event details (same logic as before)
                title = ev.get("event_name") or "(Unnamed event)"
                description = ev.get("event_description") or ""
                color = ANNOUNCEMENT_TYPES.get("event-reminder", {}).get("color", 0x2F3136)

                embed = discord.Embed(title=title, description=description, color=color)

                def _parse_time(t):
                    if not t:
                        return None
                    try:
                        if isinstance(t, str):
                            dt = parse_vatsim_logon_time(t)
                        elif isinstance(t, datetime):
                            dt = t
                        else:
                            return None
                        if dt.tzinfo is None:
                            dt = dt.replace(tzinfo=timezone.utc)
                        return dt
                    except Exception:
                        return None

                start = _parse_time(ev.get("event_start_time"))
                end = _parse_time(ev.get("event_end_time"))

                if start and end:
                    embed.add_field(name="When", value=f"<t:{int(start.timestamp())}:F> to <t:{int(end.timestamp())}:F>", inline=False)
                elif start:
                    embed.add_field(name="When", value=f"<t:{int(start.timestamp())}:F>", inline=False)
                elif end:
                    embed.add_field(name="When", value=f"Ends <t:{int(end.timestamp())}:F>", inline=False)

                ev_type = ev.get("event_type")
                host = ev.get("event_host")
                if ev_type:
                    embed.add_field(name="Type", value=str(ev_type), inline=True)
                if host:
                    embed.add_field(name="Host", value=str(host), inline=True)

                featured = ev.get("event_feature_fields") or []
                if isinstance(featured, list) and all(isinstance(x, str) for x in featured) and featured:
                    embed.add_field(name="Featured", value=", ".join(featured), inline=False)

                event_id_val = ev.get("event_id")
                if event_id_val:
                    event_url = f"https://vzdc.org/events/{event_id_val}"
                    embed.add_field(name="Event Page", value=event_url, inline=False)
                    embed.set_footer(text=f"ID: {event_id_val}")

                banner = ev.get("event_banner_url")
                if banner:
                    try:
                        embed.set_image(url=banner)
                    except Exception:
                        pass

                resp = getattr(interaction, "response", None)
                if resp and hasattr(resp, "send_message"):
                    await resp.send_message(embed=embed, ephemeral=True)
                else:
                    try:
                        await interaction.followup.send(embed=embed, ephemeral=True)
                    except Exception:
                        pass

            except Exception:
                resp = getattr(interaction, "response", None)
                if resp and hasattr(resp, "send_message"):
                    try:
                        await resp.send_message("Error handling event button.", ephemeral=True)
                    except Exception:
                        pass
                else:
                    try:
                        await interaction.followup.send("Error handling event button.", ephemeral=True)
                    except Exception:
                        pass

        # Register the listener with the bot
        try:
            bot.add_listener(_interaction_handler, "on_interaction")
            app._evt_listener_registered = True
        except Exception:
            # if registration fails, do not crash the API response
            pass

    # Attempt registration now (no-op if bot missing or handler already registered)
    try:
        _register_interaction_handler()
    except Exception:
        pass

    return jsonify({"status": "ok", "sent": len(embeds), "message_ids": result}), 200

import os.path

import discord
import requests
from flask import Blueprint, request, jsonify
import traceback
from datetime import datetime
from dateutil import parser as date_parser
from config import ANNOUNCEMENT_TYPES, IMAGE_BASE_URL
from flask import current_app

bp = Blueprint('events_api', __name__)

def format_event_time_range(start_str: str, end_str: str) -> str:
    try:
        start_dt = date_parser.parse(start_str)
        end_dt = date_parser.parse(end_str)

        date_format = "%B %d"
        if start_dt.year != datetime.utcnow().year:
            date_format += ", %Y"

        start_date_formatted = start_dt.strftime(date_format)
        end_date_formatted = end_dt.strftime(date_format)

        time_format = "%H%Mz"

        if start_dt.date() == end_dt.date():
            return f"{start_date_formatted} | {start_dt.strftime(time_format)} - {end_dt.strftime(time_format)}"
        else:
            return (
                f"{start_date_formatted} {start_dt.strftime(time_format)} - "
                f"{end_date_formatted} {end_dt.strftime(time_format)}"
            )

    except Exception as e:
        print(f"Error parsing event times {start_str} - {end_str}: {e}")
        return f"{start_str} - {end_str}"

def get_banner_url(banner_key: str) -> str:
    if not IMAGE_BASE_URL:
        print("Warning: IMAGE_BASE_URL not configured. Cannot construct banner URL.")
        return None
    return f"{IMAGE_BASE_URL}{banner_key}"


@bp.route("/create_event_post", methods=["POST"])

def create_event_post():
    app = current_app
    auth_header = request.headers.get("X-API-Key")
    if not auth_header or auth_header != app.secret_key:
        print(f"API Access Denied: Invalid X-API-Key provided: '{auth_header}'")
        return jsonify({"error": "Unauthorized", "message": "Invalid API Key"}), 401

    if not request.is_json:
        print("API Error: Request must be JSON")
        return jsonify({"error": "Request must be JSON"}), 400

    data = request.get_json()
    print(f"Received API data for event post: {data}")

    event_data = data.get("event")
    if not isinstance(event_data, dict):
        print("API Error: Missing or invalid 'event' object in payload.")
        return jsonify({"error": "Missing or invalid 'event' object in payload"}), 400

    required_event_fields = ["name", "description", "start", "end", "positions"]
    for field in required_event_fields:
        if field not in event_data or not event_data[field]:
            print(f"API Error: Missing or empty required event field: '{field}'")
            return jsonify({"error": f"Missing or empty required event field: '{field}'"}), 400

    event_title = event_data["name"]
    event_description = event_data["description"]

    event_times_formatted = format_event_time_range(event_data["start"], event_data["end"])

    event_banner_url = None
    if event_data.get("bannerKey"):
        event_banner_url = get_banner_url(event_data["bannerKey"])

    posted_positions_data = event_data.get("positions", [])

    event_config = ANNOUNCEMENT_TYPES.get("event-posting")
    if not event_config:
        print("Configuration for 'event-posting' announcement type not found in config.py.")
        return jsonify({"error": "Bot configuration error: 'event' announcement type not found."}), 500

    target_channel_id = event_config["channel_id"]
    embed_color = event_config["color"]
    title_prefix = event_config.get("title_prefix", "🗓️")

    bot_instance = app.bot
    run_discord_op = app.run_discord_op  # Access run_discord_op here

    try:
        run_discord_op(bot_instance.wait_until_ready())

        channel = run_discord_op(bot_instance.fetch_channel(target_channel_id))
        if not channel:
            print(f"API Error: Target channel with ID {target_channel_id} not found or inaccessible.")
            return jsonify({"error": f"Target channel with ID {target_channel_id} not found or inaccessible."}), 404

        if not isinstance(channel, discord.TextChannel):
            print(f"API Error: Channel {target_channel_id} is not a text channel.")
            return jsonify(
                {"error": f"Channel {target_channel_id} is not a text channel where messages can be sent."}), 400

        embed = discord.Embed(
            title=f"{title_prefix} {event_title}".strip(),
            description=event_description,
            color=embed_color,
            timestamp=datetime.utcnow()
        )

        embed.add_field(name="⏰ Event Times", value=event_times_formatted, inline=False)

        positions_lines = []
        all_discord_mentions = []

        if posted_positions_data:
            posted_positions_data.sort(key=lambda x: x.get("finalPosition", x.get("requestedPosition", "")))
            for pos_item in posted_positions_data:

                if not pos_item.get("published", False):
                    continue

                pos_name = pos_item.get("finalPosition", pos_item.get("requestedPosition", "N/A Position"))
                user_data = pos_item.get("user")

                controller_display = "(Open)"
                if user_data:
                    assigned_controller_name = user_data.get("fullName") or \
                                               (
                                                   f"{user_data.get('firstName', '')} {user_data.get('lastName', '')}").strip()
                    assigned_discord_uid = user_data.get("discordUid")

                    if assigned_controller_name:
                        controller_display = assigned_controller_name
                        if assigned_discord_uid:
                            try:
                                controller_display += f" (<@{int(assigned_discord_uid)}>)"
                                all_discord_mentions.append(f"<@{int(assigned_discord_uid)}>")
                            except ValueError:
                                print(f"Invalid Discord UID for {pos_name}: {assigned_discord_uid}")

                pos_time_range = format_event_time_range(pos_item.get("finalStartTime", ""), pos_item.get("finalEndTime", ""))
                controller_time_window = f"-- *{pos_time_range}*" if event_times_formatted != pos_time_range else ""
                positions_lines.append(f"• **{pos_name}**: {controller_display} {controller_time_window}")

            positions_value = "\n".join(positions_lines)
            embed.add_field(name="📍 Posted Positions", value=positions_value, inline=False)

        banner_key = event_data.get("bannerKey", "default_banner")
        if event_banner_url:
            # Fetch the image from the URL
            response = requests.get(event_banner_url, stream=True)
            response.raise_for_status()

            # Save the image temporarily
            temp_image_path = f"{banner_key}.png"
            with open(temp_image_path, "wb") as temp_file:
                for chunk in response.iter_content(1024):
                    temp_file.write(chunk)

            embed.set_image(url=f"attachment://{banner_key}.png")

        embed.set_footer(text="Automated Event Post")

        ping_message = " ".join(all_discord_mentions) if all_discord_mentions else "Heads up everyone!"

        message = run_discord_op(channel.send(content=ping_message, embed=embed))
        if os.path.exists(f"{banner_key}.png"):
            os.remove(f"{banner_key}.png")

        print(f"Successfully posted event announcement to channel {channel.name} (ID: {channel.id})")

        return jsonify({
            "status": "success",
            "message": "Event announcement posted successfully",
            "channel_id": channel.id,
            "message_id": message.id,
            "url": message.jump_url
        }), 200

    except Exception as e:
        print(f"API Error during event posting: {e}")
        traceback.print_exc()
        return jsonify({
            "status": "error",
            "code": 500,
            "message": str(e)
        }), 500
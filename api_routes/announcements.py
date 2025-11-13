from flask import Blueprint, jsonify, request, current_app as app
import discord
from bot import logger
import config as cfg

bp = Blueprint('announcements', __name__)


@bp.route('/announcements', methods=['POST'])
def handle_announcements():
    """Catch-all endpoint for posting announcements into Discord.

    Expected JSON body:
      - message_type: str (one of cfg.ANNOUNCEMENT_TYPES keys)
      - title: str
      - body: str
    Optional:
      - channel_id: int (overrides configured channel)
      - author_name, author_rating, author_staff_position, banner_url, event_id

    The endpoint will schedule a coroutine on the bot event loop via
    app.run_discord_op to send the message to the appropriate channel.
    """
    # API key check
    auth_header = request.headers.get("X-API-Key")
    if not auth_header or auth_header != app.secret_key:
        logger.info(f"API Access Denied: Invalid X-API-Key provided: '{auth_header}'")
        return jsonify({"error": "Invalid X-API-Key provided"}), 401

    if not request.is_json:
        logger.info("API Access Denied: Request content type is not JSON")
        return jsonify({"error": "Request must be in JSON format"}), 400

    data = request.get_json()

    required_fields = ["message_type", "title", "body"]
    for field in required_fields:
        if field not in data:
            logger.info(f"API Access Denied: Missing required field '{field}' in request data")
            return jsonify({"error": f"Missing required field: {field}"}), 400

    message_type = data.get("message_type").lower()
    title = data.get("title")
    body = data.get("body")
    channel_override = data.get("channel_id")

    author_name = data.get("author_name")
    author_rating = data.get("author_rating")
    author_staff_position = data.get("author_staff_position")
    banner_url = data.get("banner_url")
    event_id = data.get("event_id")
    dry_run = data.get("dry_run", False)

    # Validate message type and resolve target channel & embed properties
    if message_type not in cfg.ANNOUNCEMENT_TYPES:
        logger.info(f"API Access Denied: Unsupported message_type '{message_type}'")
        return jsonify({"error": f"Unsupported message_type: {message_type}"}), 400

    announce_config = cfg.ANNOUNCEMENT_TYPES[message_type]
    target_channel_id = channel_override or announce_config.get("channel_id")
    color_value = announce_config.get("color")
    title_prefix = announce_config.get("title_prefix", "")

    # Build embed
    embed = discord.Embed(
        title=f"{title_prefix} {title}".strip(),
        description=body,
        color=discord.Color(color_value) if color_value is not None else discord.Color.default()
    )

    if author_name:
        embed.set_author(name=author_name)

    # Add optional fields in the embed footer
    footer_parts = []
    if author_staff_position:
        footer_parts.append(str(author_staff_position))
    if author_rating:
        footer_parts.append(str(author_rating))
    if footer_parts:
        embed.set_footer(text=" | ".join(footer_parts))

    if banner_url:
        embed.set_image(url=banner_url)

    # If caller requested a dry run, return the built message payload without sending to Discord.
    if dry_run:
        embed_payload = {
            "title": embed.title,
            "description": embed.description,
            "color": color_value,
            "author": embed.author.name if embed.author else None,
            "footer": embed.footer.text if embed.footer else None,
            "image_url": banner_url,
            "message_type": message_type,
            "target_channel_id": target_channel_id,
            "event_id": event_id,
        }
        logger.info(f"Dry-run announcement prepared (type={message_type}): {embed_payload}")
        return jsonify({"status": "dry_run", "payload": embed_payload}), 200

    # Prepare coroutine to send message
    async def _send():
        bot = getattr(app, "bot", None)
        if bot is None:
            raise RuntimeError("Discord bot instance not available on Flask app")

        # Try to get channel by cache first, then fetch if necessary
        channel = bot.get_channel(int(target_channel_id)) if target_channel_id is not None else None
        if channel is None:
            try:
                channel = await bot.fetch_channel(int(target_channel_id))
            except Exception as e:
                logger.exception(f"Failed to fetch channel {target_channel_id}: {e}")
                raise

        # Send the embed
        sent = await channel.send(embed=embed)
        return sent.id

    # Run the coroutine on the bot event loop
    try:
        run_op = getattr(app, "run_discord_op", None)
        if run_op is None:
            raise RuntimeError("API server helper 'run_discord_op' not available on app; is the bot running?")
        message_id = run_op(_send())
        logger.info(f"Posted announcement (type={message_type}) to channel {target_channel_id} as message {message_id}")
        return jsonify({"status": "ok", "channel_id": target_channel_id, "message_id": message_id}), 200
    except Exception as e:
        logger.exception(f"Failed to post announcement: {e}")
        return jsonify({"error": "Failed to post announcement", "detail": str(e)}), 500


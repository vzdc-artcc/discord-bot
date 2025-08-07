# extensions/announcements_api.py
import discord
from discord.ext import commands
from flask import Flask, request, jsonify
from waitress import serve
import asyncio
import threading
import logging
import traceback
from datetime import datetime

from config import API_SECRET_KEY, API_PORT, ANNOUNCEMENT_TYPES, atc_rating

log = logging.getLogger('werkzeug')
log.setLevel(logging.ERROR)
app = Flask(__name__)

def run_discord_op(coro):

    if not hasattr(app, 'bot_loop') or app.bot_loop is None:
        raise RuntimeError("Bot event loop not set up in Flask app. Is the bot ready?")

    future = asyncio.run_coroutine_threadsafe(coro, app.bot_loop)
    return future.result()


class AnnouncementsAPI(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.api_running = False
        self.api_thread = None

        app.bot = self.bot
        app.bot_loop = None
        app.secret_key = API_SECRET_KEY

    @commands.Cog.listener()
    async def on_ready(self):
        if not self.api_running:
            print(f"Starting Announcements API listener on port {API_PORT}...")
            app.bot_loop = asyncio.get_event_loop()

            self.api_thread = threading.Thread(target=self._run_flask_app)
            self.api_thread.daemon = True
            self.api_thread.start()
            self.api_running = True
            print("Announcements API listener thread started.")

    def _run_flask_app(self):
        serve(app, host="0.0.0.0", port=API_PORT)

    @commands.command(name="restartannapi")
    @commands.is_owner()
    async def restart_ann_api(self, ctx):
        await ctx.send(
            "Announcements API server cannot be gracefully restarted directly. Please restart the entire bot process.")

@app.route("/announcement", methods=["POST"])
def post_announcement_api():

    auth_header = request.headers.get("X-API-Key")
    if not auth_header or auth_header != app.secret_key:
        print(f"API Access Denied: Invalid X-API-Key provided: '{auth_header}'")
        return jsonify({"error": "Unauthorized"}), 401

    if not request.is_json:
        print("API Error: Request must be JSON")
        return jsonify({"error": "Request must be JSON"}), 400

    data = request.get_json()
    print(f"Received API data for announcement: {data}")

    required_fields = ["message_type", "title", "body"]
    for field in required_fields:
        if field not in data or not data[field]:
            print(f"API Error: Missing or empty required field: '{field}'")
            return jsonify({"error": f"Missing or empty required field: '{field}'"}), 400

    message_type = data["message_type"].lower()
    title = data["title"]
    body = data["body"]
    author_name = data.get("author")
    author_rating_code = data.get("author_rating")
    author_staff_position = data.get("author_staff_position")

    announcement_config = ANNOUNCEMENT_TYPES.get(message_type)
    if not announcement_config:
        print(
            f"API Error: Invalid message_type provided: '{message_type}'. Must be one of {list(ANNOUNCEMENT_TYPES.keys())}")
        return jsonify(
            {"error": f"Invalid message_type: '{message_type}'. Must be one of {list(ANNOUNCEMENT_TYPES.keys())}"}), 400

    target_channel_id = announcement_config["channel_id"]
    embed_color = announcement_config["color"]
    title_prefix = announcement_config.get("title_prefix", "")

    bot_instance = app.bot

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
            title=f"{title_prefix} {title}".strip(),
            description=body,
            color=embed_color
        )

        author_info = []
        if author_name:
            author_info.append(author_name)
        if author_rating_code is not None:
            if isinstance(author_rating_code, int):
                rating_str = atc_rating.get(author_rating_code, f"Rating {author_rating_code}")
                author_info.append(f"({rating_str})")
            else:
                author_info.append(f"({author_rating_code})")
        if author_staff_position:
            author_info.append(f"[{author_staff_position}]")

        if author_info:
            embed.set_footer(text=f"By: {' '.join(author_info)}")

        embed.timestamp = datetime.utcnow()

        message = run_discord_op(channel.send(embed=embed))
        print(f"Successfully posted announcement to channel {channel.name} (ID: {channel.id})")

        return jsonify({
            "status": "success",
            "message": "Announcement posted successfully",
            "channel_id": channel.id,
            "message_id": message.id,
            "url": message.jump_url
        }), 201

    except Exception as e:
        print(f"API Error during announcement posting: {e}")
        traceback.print_exc()
        return jsonify({
            "status": "error",
            "code": 500,
            "message": str(e)
        }), 500


async def setup(bot):
    await bot.add_cog(AnnouncementsAPI(bot))
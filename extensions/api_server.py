from discord.ext import commands
from flask import Flask, jsonify, request
from waitress import serve
import asyncio
import threading
import logging
import importlib
from config import API_SECRET_KEY, API_PORT
from bot import logger  # use the project logger instead of prints

# Reduce werkzeug logging noise but keep the project's logger for messages
werkzeug_logger = logging.getLogger('werkzeug')
werkzeug_logger.setLevel(logging.ERROR)

app = Flask(__name__)

# Attach attributes used elsewhere so static analyzers won't complain.
# These get set properly when the cog is initialized.
app.bot = None  # type: ignore[attr-defined]
app.bot_loop = None  # type: ignore[attr-defined]

def run_discord_op(coro):
    if not hasattr(app, 'bot_loop') or app.bot_loop is None:
        logger.error("Bot event loop not set up in Flask app. Is the bot ready?")
        raise RuntimeError("Bot event loop not set up in Flask app. Is the bot ready?")

    future = asyncio.run_coroutine_threadsafe(coro, app.bot_loop)
    return future.result()


app.run_discord_op = run_discord_op
app.secret_key = API_SECRET_KEY


class APIServer(commands.Cog):  # Renamed to APIServer
    def __init__(self, bot):
        self.bot = bot
        self.api_running = False
        self.api_thread = None

        app.bot = self.bot
        app.bot_loop = None

        # Register the actual available blueprints in api_routes
        self.blueprint_modules = [
            "api_routes.announcements",
            "api_routes.event_position_posting",
            "api_routes.regular_event_reminder",
        ]
        self._register_blueprints()

    def _register_blueprints(self):
        for module_name in self.blueprint_modules:
            try:
                module = importlib.import_module(module_name)
                if hasattr(module, 'bp'):
                    app.register_blueprint(module.bp)
                    logger.info(f"Registered Flask Blueprint: {module_name}")
                else:
                    logger.warning(f"Module {module_name} does not have a 'bp' attribute.")
            except ImportError as e:
                logger.error(f"Error importing blueprint module {module_name}: {e}")
            except Exception as e:
                logger.exception(f"Error registering blueprint {module_name}: {e}")

    @commands.Cog.listener()
    async def on_ready(self):
        if not self.api_running:
            logger.info(f"Starting API Server listener on port {API_PORT}...")
            # use the bot's event loop when available
            try:
                app.bot_loop = asyncio.get_event_loop()
            except Exception as e:
                logger.exception("Failed to get event loop for API server: %s", e)
                app.bot_loop = None

            self.api_thread = threading.Thread(target=self._run_flask_app)
            self.api_thread.daemon = True
            self.api_thread.start()
            self.api_running = True
            logger.info("API Server listener thread started.")
        else:
            logger.debug("API Server already running; on_ready called again.")

    def _run_flask_app(self):
        try:
            serve(app, host="0.0.0.0", port=API_PORT)
        except Exception as e:
            logger.exception(f"API server failed to start: {e}")

    @commands.command(name="restartapi")
    @commands.is_owner()
    async def restart_api_server(self, ctx):
        logger.info("Restart API requested by %s (%s)", ctx.author, ctx.author.id)
        await ctx.send("API Server cannot be gracefully restarted directly. Please restart the entire bot process.")


def api_key_required(f):
    def decorated_function(*args, **kwargs):
        auth_header = request.headers.get("X-API-Key")
        if not auth_header or auth_header != app.secret_key:
            logger.warning("Unauthorized API request from %s", request.remote_addr)
            return jsonify({"error": "Unauthorized", "message": "Invalid or missing API key"}), 401
        return f(*args, **kwargs)

    decorated_function.__name__ = f.__name__
    return decorated_function

async def setup(bot):
    await bot.add_cog(APIServer(bot))
    # The bot's load_extensions function will load other cogs in the extensions folder.

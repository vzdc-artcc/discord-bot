from discord.ext import commands
from flask import Flask, jsonify, request  # Only jsonify needed for generic responses
from waitress import serve
import asyncio
import threading
import logging
import importlib
from config import API_SECRET_KEY, API_PORT

log = logging.getLogger('werkzeug')
log.setLevel(logging.ERROR)

app = Flask(__name__)

def run_discord_op(coro):
    if not hasattr(app, 'bot_loop') or app.bot_loop is None:
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

        self.blueprint_modules = [
            "api_routes.announcements",
            "api_routes.events",
            "api_routes.training"
        ]
        self._register_blueprints()

    def _register_blueprints(self):
        for module_name in self.blueprint_modules:
            try:
                module = importlib.import_module(module_name)
                if hasattr(module, 'bp'):
                    app.register_blueprint(module.bp)
                    print(f"Registered Flask Blueprint: {module_name}")
                else:
                    print(f"Warning: Module {module_name} does not have a 'bp' attribute.")
            except ImportError as e:
                print(f"Error importing blueprint module {module_name}: {e}")
            except Exception as e:
                print(f"Error registering blueprint {module_name}: {e}")

    @commands.Cog.listener()
    async def on_ready(self):
        if not self.api_running:
            print(f"Starting API Server listener on port {API_PORT}...")
            app.bot_loop = asyncio.get_event_loop()

            self.api_thread = threading.Thread(target=self._run_flask_app)
            self.api_thread.daemon = True
            self.api_thread.start()
            self.api_running = True
            print("API Server listener thread started.")

    def _run_flask_app(self):
        serve(app, host="0.0.0.0", port=API_PORT)

    @commands.command(name="restartapi")
    @commands.is_owner()
    async def restart_api_server(self, ctx):
        await ctx.send("API Server cannot be gracefully restarted directly. Please restart the entire bot process.")

def api_key_required(f):
    def decorated_function(*args, **kwargs):
        auth_header = request.headers.get("X-API-Key")
        if not auth_header or auth_header != app.secret_key:
            return jsonify({"error": "Unauthorized", "message": "Invalid or missing API key"}), 401
        return f(*args, **kwargs)

    decorated_function.__name__ = f.__name__
    return decorated_function

async def setup(bot):
    await bot.add_cog(APIServer(bot))
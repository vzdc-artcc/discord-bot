import discord
from flask import Blueprint, request, jsonify
from flask import current_app
from config import TRAINING_CATEGORY_ID
import traceback
import os
import json

bp = Blueprint("training_api", __name__)

TRAINING_CHANNELS_FILE = f"{os.getcwd()}/data/training_channels.json"


def load_training_channels():
    if os.path.exists(TRAINING_CHANNELS_FILE):
        with open(TRAINING_CHANNELS_FILE, "r") as f:
            try:
                return json.load(f)
            except json.JSONDecodeError:
                return {}
    return {}


def save_training_channels(data):
    os.makedirs(os.path.dirname(TRAINING_CHANNELS_FILE), exist_ok=True)
    with open(TRAINING_CHANNELS_FILE, "w") as f:
        json.dump(data, f)


@bp.route("/create_training_channel", methods=["POST"])
async def create_training_channel():
    app = current_app
    auth_header = request.headers.get("X-API-Key")
    if not auth_header or auth_header != app.secret_key:
        print(f"API Access Denied: Invalid X-API-Key provided: '{auth_header}'")
        return (
            jsonify({"error": "Unauthorized", "message": "Invalid API Key"}),
            401,
        )

    if not request.is_json:
        print("API Error: Request must be JSON")
        return jsonify({"error": "Request must be JSON"}), 400

    data = request.get_json()
    print(f"Received API data for training channel post: {data}")

    # Validate student object
    student = data.get("student")
    if not isinstance(student, dict):
        return jsonify({"error": "Missing or invalid 'student' object"}), 400

    first_name = student.get("firstName")
    last_name = student.get("lastName")
    cid = student.get("cid")
    if not all([first_name, last_name, cid]):
        return jsonify({"error": "Student firstName, lastName, and cid are required"}), 400

    # Load existing training channels
    training_channels = load_training_channels()

    # Check if channel already exists for this CID
    if cid in training_channels:
        existing_channel_id = training_channels[cid]
        return (
            jsonify(
                {
                    "error": "Channel already exists",
                    "message": f"A training channel for {first_name} {last_name} ({cid}) already exists.",
                    "channel_id": existing_channel_id,
                }
            ),
            409,
        )

    # Primary trainer
    primary_trainer = data.get("primaryTrainer")
    if not isinstance(primary_trainer, dict):
        return jsonify({"error": "Missing or invalid 'primaryTrainer' object"}), 400

    # Other trainers
    other_trainers = data.get("otherTrainers", [])
    if not isinstance(other_trainers, list):
        return jsonify({"error": "'otherTrainers' must be a list"}), 400

    if not TRAINING_CATEGORY_ID:
        print("Warning: TRAINING_CATEGORY_ID not configured")
        return jsonify({"error": "Training Category ID not configured"}), 500

    bot_instance = app.bot
    run_discord_op = app.run_discord_op

    try:
        run_discord_op(bot_instance.wait_until_ready())

        category = run_discord_op(bot_instance.fetch_channel(TRAINING_CATEGORY_ID))
        if not category or not isinstance(category, discord.CategoryChannel):
            return jsonify({"error": "Training category not found"}), 404

        overwrites = {}

        mention_ids = []

        def add_user_overwrite(user_id):
            if not user_id:
                return
            try:
                uid_int = int(user_id)
                mention_ids.append(f"<@{uid_int}>")  # Always add mention string

                member = category.guild.get_member(uid_int)
                if member:
                    overwrites[member] = discord.PermissionOverwrite(
                        view_channel=True, send_messages=True, read_message_history=True
                    )
                else:
                    # Use discord.Object for uncached members
                    overwrites[discord.Object(id=uid_int)] = discord.PermissionOverwrite(
                        view_channel=True, send_messages=True, read_message_history=True
                    )
                    print(f"User with ID {user_id} not found in cache, using discord.Object.")
            except ValueError:
                print(f"Invalid Discord UID: {user_id}")

        add_user_overwrite(student.get("discordUid"))
        add_user_overwrite(primary_trainer.get("discordUid"))
        for trainer in other_trainers:
            add_user_overwrite(trainer.get("discordUid"))

        # Normalize channel name for Discord
        raw_channel_name = f"{first_name} {last_name} - {cid}"
        normalized_channel_name = raw_channel_name.lower().replace(" ", "-")

        # Create the channel
        new_channel = run_discord_op(
            category.guild.create_text_channel(
                name=normalized_channel_name, category=category, overwrites=overwrites
            )
        )

        await new_channel.edit(sync_permissions=True)
        await new_channel.edit(overwrites=overwrites)

        print(f"Created training channel '{new_channel.name}' (ID: {new_channel.id})")

        # Save channel ID for this CID
        training_channels[cid] = new_channel.id
        save_training_channels(training_channels)

        # Send welcome embed
        embed = discord.Embed(
            title="📚 New Training Session",
            description=(
                f"Welcome to the training channel for **{first_name} {last_name} ({cid})**.\n\n"
                "This channel is for coordination between the student and assigned trainers.\n"
                "Please use it to share training materials, schedule sessions, and ask questions."
            ),
            color=discord.Color.green(),
        )
        embed.add_field(
            name="Student",
            value=f"{first_name} {last_name} ({cid})",
            inline=False,
        )
        embed.add_field(
            name="Primary Trainer",
            value=primary_trainer.get("fullName", "N/A"),
            inline=False,
        )
        if other_trainers:
            embed.add_field(
                name="Other Trainers",
                value="\n".join(t.get("fullName", "N/A") for t in other_trainers),
                inline=False,
            )
        embed.set_footer(text="Training Coordination Channel")

        mention_str = " ".join(mention_ids) if mention_ids else None
        run_discord_op(new_channel.send(content=mention_str, embed=embed))

        return (
            jsonify(
                {
                    "status": "success",
                    "channel_id": new_channel.id,
                    "channel_name": new_channel.name,
                }
            ),
            201,
        )

    except Exception as e:
        print(f"Error creating training channel: {e}")
        traceback.print_exc()
        return jsonify({"status": "error", "message": str(e)}), 500
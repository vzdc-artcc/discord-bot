import discord
from discord.ext import commands
import json
import os
from config import BREAK_BOARD_CHANNEL_ID, BREAK_BOARD_ROLE_MAP
import re

MESSAGE_ID_FILE = "data/notification_message_id.json"


class BreakRequestActions(discord.ui.View):
    def __init__(self, request_user_id: int):
        super().__init__(timeout=3600)
        self.request_user_id = request_user_id

    async def on_timeout(self):
        for item in self.children:
            item.disabled = True
        if self.message:
            await self.message.edit(view=self)

    async def on_error(self, interaction: discord.Interaction, error: Exception, item: discord.ui.Item):
        print(f"Error during BreakRequestActions interaction for {item.custom_id}: {error}")
        if not interaction.response.is_done():
            await interaction.response.send_message("An error occurred with this action.", ephemeral=True)
        else:
            await interaction.followup.send("An error occurred after acknowledging this action.", ephemeral=True)

    @discord.ui.button(label="Claim Position", style=discord.ButtonStyle.success, custom_id="claim_break_position")
    async def claim_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await interaction.response.defer()

        reliever_user = interaction.user
        requesting_user = interaction.guild.get_member(self.request_user_id)

        if requesting_user:
            await interaction.channel.send(
                f"🚨 {reliever_user.mention} has claimed the break for {requesting_user.mention}! "
                "Please coordinate directly."
            )
        else:
            await interaction.channel.send(
                f"🚨 {reliever_user.mention} has claimed this break! The original requester is no longer in the server."
            )

        for item in self.children:
            item.disabled = True
        await interaction.message.edit(view=self)

        await interaction.followup.send(f"You have successfully claimed this break. The requester has been notified.",
                                        ephemeral=True)

    @discord.ui.button(label="Done / Delete", style=discord.ButtonStyle.danger, custom_id="delete_break_request")
    async def delete_button(self, interaction: discord.Interaction, button: discord.ui.Button):

        await interaction.response.defer(ephemeral=True)

        is_requester = interaction.user.id == self.request_user_id

        can_delete = is_requester or interaction.user.guild_permissions.manage_messages

        if not can_delete:
            await interaction.followup.send("You are not authorized to delete this message.", ephemeral=True)
            return

        try:
            await interaction.message.delete()
            print(f"Break request message deleted by {interaction.user.name}.")

        except discord.Forbidden:
            await interaction.followup.send("I don't have permission to delete this message.", ephemeral=True)
        except Exception as e:
            await interaction.followup.send(f"Failed to delete message: {e}", ephemeral=True)
            print(f"Error deleting break request message: {e}")


class BreakTimeModal(discord.ui.Modal, title="Break Request Details"):
    def __init__(self, bot_instance: commands.Bot, role_name: str, role_id: int):
        super().__init__()
        self.bot_instance = bot_instance
        self.role_name = role_name
        self.role_id = role_id

    time_input = discord.ui.TextInput(
        label="How long can you wait for relief?",
        placeholder="e.g., 15 minutes, 30m, 1h, 1hr 30min",
        required=False,
        max_length=50,
        custom_id="break_time_input"
    )

    async def on_submit(self, interaction: discord.Interaction):
        await interaction.response.defer(ephemeral=True)

        wait_time_raw = self.time_input.value.strip()

        wait_time_display = "no specific time"
        if wait_time_raw:
            if re.match(r"^\d+\s*(m|min|minute(s)?|h|hr|hour(s)?)((\s+)?\d+\s*(m|min|minute(s)?))?$", wait_time_raw,
                        re.IGNORECASE):
                wait_time_display = f"for **{wait_time_raw}**"
            else:
                await interaction.followup.send(
                    f"Invalid time format: `{wait_time_raw}`. Please use formats like '15 minutes', '1h', '30m'. "
                    f"Sending request without specific time.", ephemeral=True
                )
                wait_time_display = "no specific time"

        break_board_cog = self.bot_instance.get_cog("BreakBoard")
        if break_board_cog:
            await break_board_cog.send_notification(
                interaction,
                self.role_name,
                self.role_id,
                wait_time_display
            )
        else:
            print("Error: BreakBoard cog not found during modal submission.")
            await interaction.followup.send(
                "An internal error occurred: BreakBoard cog not found.", ephemeral=True
            )

    async def on_error(self, interaction: discord.Interaction, error: Exception):
        print(f"Error submitting modal for {self.role_name}: {error}")
        if not interaction.response.is_done():
            await interaction.response.send_message("An error occurred while processing your request.", ephemeral=True)
        else:
            await interaction.followup.send("An error occurred after acknowledging your request.", ephemeral=True)


class BreakBoardButtons(discord.ui.View):
    def __init__(self, bot):
        super().__init__(timeout=None)
        self.bot = bot

    async def on_error(self, interaction: discord.Interaction, error: Exception, item: discord.ui.Item):
        print(f"Error during button interaction for {item.custom_id}: {error}")
        if not interaction.response.is_done():
            await interaction.response.send_message("An error occurred with this button.", ephemeral=True)
        else:
            await interaction.followup.send("An error occurred after acknowledging this button click.", ephemeral=True)

    @discord.ui.button(label="Unrestricted GND", style=discord.ButtonStyle.blurple, custom_id="gnd_unrestricted")
    async def gnd_unrestricted_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        modal = BreakTimeModal(self.bot, "Unrestricted GND", BREAK_BOARD_ROLE_MAP["gnd_unrestricted"])
        await interaction.response.send_modal(modal)

    @discord.ui.button(label="Tier 1 GND", style=discord.ButtonStyle.blurple, custom_id="gnd_tier1")
    async def gnd_tier1_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        modal = BreakTimeModal(self.bot, "Tier 1 GND", BREAK_BOARD_ROLE_MAP["gnd_tier1"])
        await interaction.response.send_modal(modal)

    @discord.ui.button(label="Unrestricted TWR", style=discord.ButtonStyle.blurple, custom_id="twr_unrestricted")
    async def twr_unrestricted_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        modal = BreakTimeModal(self.bot, "Unrestricted TWR", BREAK_BOARD_ROLE_MAP["twr_unrestricted"])
        await interaction.response.send_modal(modal)

    @discord.ui.button(label="Tier 1 TWR", style=discord.ButtonStyle.blurple, custom_id="twr_tier1")
    async def twr_tier1_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        modal = BreakTimeModal(self.bot, "Tier 1 TWR", BREAK_BOARD_ROLE_MAP["twr_tier1"])
        await interaction.response.send_modal(modal)

    @discord.ui.button(label="Unrestricted APP", style=discord.ButtonStyle.blurple, custom_id="app_unrestricted")
    async def app_unrestricted_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        modal = BreakTimeModal(self.bot, "Unrestricted APP", BREAK_BOARD_ROLE_MAP["app_unrestricted"])
        await interaction.response.send_modal(modal)

    @discord.ui.button(label="PCT", style=discord.ButtonStyle.blurple, custom_id="pct")
    async def pct_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        modal = BreakTimeModal(self.bot, "PCT", BREAK_BOARD_ROLE_MAP["pct"])
        await interaction.response.send_modal(modal)

    @discord.ui.button(label="Center", style=discord.ButtonStyle.blurple, custom_id="center")
    async def center_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        modal = BreakTimeModal(self.bot, "Center", BREAK_BOARD_ROLE_MAP["center"])
        await interaction.response.send_modal(modal)


class BreakBoard(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.message_id = None
        self.channel_id = BREAK_BOARD_CHANNEL_ID

        print(f"DEBUG(BreakBoard): Initializing cog. MESSAGE_ID_FILE: {MESSAGE_ID_FILE}")
        if os.path.exists(MESSAGE_ID_FILE):
            print(f"DEBUG(BreakBoard): {MESSAGE_ID_FILE} found. Attempting to load...")
            try:
                with open(MESSAGE_ID_FILE, "r") as f:
                    data = json.load(f)
                    self.message_id = data.get("message_id")
                    saved_channel_id = data.get("channel_id")

                    print(
                        f"DEBUG(BreakBoard): Loaded message_id={self.message_id}, saved_channel_id={saved_channel_id}")
                    print(f"DEBUG(BreakBoard): Configured channel_id={self.channel_id}")

                    if saved_channel_id is None:
                        print("WARNING(BreakBoard): 'channel_id' not found in saved data. Will re-evaluate.")
                        # If channel_id wasn't saved before, don't reset message_id yet, let on_ready decide
                    elif saved_channel_id != self.channel_id:
                        print(f"WARNING(BreakBoard): Notification channel ID mismatch! "
                              f"Saved: {saved_channel_id}, Config: {self.channel_id}. Resetting message_id.")
                        self.message_id = None
            except json.JSONDecodeError as e:
                print(f"ERROR(BreakBoard): Corrupted JSON file {MESSAGE_ID_FILE}: {e}. Resetting message_id.")
                self.message_id = None
            except Exception as e:
                print(f"ERROR(BreakBoard): Unexpected error loading {MESSAGE_ID_FILE}: {e}. Resetting message_id.")
                self.message_id = None
        else:
            print(f"DEBUG(BreakBoard): {MESSAGE_ID_FILE} not found. Creating data directory if needed.")
            os.makedirs(os.path.dirname(MESSAGE_ID_FILE), exist_ok=True)
            print(f"DEBUG(BreakBoard): Data directory created/exists: {os.path.dirname(MESSAGE_ID_FILE)}")

    def save_message_id(self, message_id: int):
        self.message_id = message_id
        try:
            with open(MESSAGE_ID_FILE, "w") as f:
                json.dump({"message_id": message_id, "channel_id": self.channel_id}, f)
            print(
                f"DEBUG(BreakBoard): Saved message_id={message_id}, channel_id={self.channel_id} to {MESSAGE_ID_FILE}")
        except Exception as e:
            print(f"ERROR(BreakBoard): Failed to save message_id to file {MESSAGE_ID_FILE}: {e}")

    @commands.Cog.listener()
    async def on_ready(self):
        if not self.bot.is_ready():
            print("DEBUG(BreakBoard): Bot not fully ready yet. Skipping on_ready for now.")
            return

        print("DEBUG(BreakBoard): BreakBoard cog on_ready started.")
        channel = self.bot.get_channel(self.channel_id)
        if not channel:
            print(f"ERROR(BreakBoard): BreakBoard channel with ID {self.channel_id} not found or inaccessible to bot.")
            print("DEBUG(BreakBoard): This usually means the bot can't see the channel or the ID is wrong.")
            return

        print(f"DEBUG(BreakBoard): Channel object found: {channel.name} ({channel.id}). Type: {type(channel)}")

        if self.message_id:
            print(f"DEBUG(BreakBoard): message_id is set ({self.message_id}). Attempting to fetch existing message.")
            try:
                # Add a sanity check for channel permissions *before* fetching
                bot_member = channel.guild.get_member(self.bot.user.id)
                if not bot_member:
                    print("ERROR(BreakBoard): Bot user not found in guild. Cannot check permissions.")
                    self.message_id = None  # Reset, as we can't manage
                else:
                    perms = channel.permissions_for(bot_member)
                    if not perms.read_message_history:
                        print(
                            f"ERROR(BreakBoard): Bot lacks 'Read Message History' in {channel.name}. Cannot fetch message.")
                        self.message_id = None
                    if not perms.view_channel:
                        print(f"ERROR(BreakBoard): Bot lacks 'View Channel' in {channel.name}. Cannot fetch message.")
                        self.message_id = None

                if self.message_id is not None:  # Check again after potential permission reset
                    message = await channel.fetch_message(self.message_id)
                    self.bot.add_view(BreakBoardButtons(self.bot), message_id=message.id)
                    print(
                        f"SUCCESS(BreakBoard): Found existing BreakBoard message (ID: {self.message_id}). Re-attached view.")
                    return  # Exit as message was found and view re-attached
            except discord.NotFound:
                print(
                    f"DEBUG(BreakBoard): Previous BreakBoard message {self.message_id} not found on Discord. It might have been deleted.")
                self.message_id = None
            except discord.Forbidden:
                print(
                    f"ERROR(BreakBoard): Bot is forbidden from fetching message {self.message_id} in channel {self.channel_id}.")
                print(
                    "DEBUG(BreakBoard): This could be due to role hierarchy, channel overwrites, or missing permissions.")
                self.message_id = None
            except Exception as e:
                print(f"ERROR(BreakBoard): An unexpected error occurred while fetching message {self.message_id}: {e}")
                self.message_id = None

        print("DEBUG(BreakBoard): message_id is None or fetch failed. Sending a new initial embed.")
        await self.send_initial_embed_with_buttons(channel)

    async def send_notification(self, interaction: discord.Interaction, role_name: str, role_id: int,
                                wait_time: str = "no specific time"):

        print(
            f"DEBUG(BreakBoard): Sending notification for {role_name} (Role ID: {role_id}) requested by {interaction.user.name}")
        role = interaction.guild.get_role(role_id)
        if not role:
            print(f"ERROR(BreakBoard): Role '{role_name}' (ID: {role_id}) not found in guild.")
            await interaction.followup.send(
                f"Error: Role for '{role_name}' not found. Please contact an administrator.", ephemeral=True)
            return

        user_mention = interaction.user.mention
        message_to_send = (
            f"{role.mention} **{role_name} break request!** "
            f"Controller {user_mention} is requesting a break for {role_name} {wait_time}."
        )

        try:
            notification_channel = interaction.channel
            dynamic_view = BreakRequestActions(interaction.user.id)

            sent_message = await notification_channel.send(message_to_send, view=dynamic_view)
            dynamic_view.message = sent_message
            print(
                f"DEBUG(BreakBoard): Notification message sent to channel {notification_channel.name} (ID: {sent_message.id})")

            await interaction.followup.send(
                f"Notification sent for {role_name}! You indicated you can wait {wait_time}. "
                "This message will be deleted automatically if claimed/resolved. "
                "You or the relieving controller can click 'Done / Delete' to remove it.",
                ephemeral=True
            )
        except Exception as e:
            print(f"ERROR(BreakBoard): Failed to send notification message for {role_name}: {e}")
            await interaction.followup.send(f"Failed to send notification: {e}", ephemeral=True)
            print(f"Failed to send notification for {role_name} (Role ID: {role_id}): {e}")

    async def send_initial_embed_with_buttons(self, channel: discord.TextChannel):
        print(f"DEBUG(BreakBoard): Attempting to send initial embed to channel {channel.name} ({channel.id}).")
        embed = discord.Embed(
            title="Controller Break Notification System",
            description=(
                "Use the buttons below to request a break for specific positions.\n"
                "- Once your request is picked up by another controller, or if it's no longer needed, "
                "**please delete the notification message** to keep this channel clear.\n "
                "- The message will include a 'Claim' and 'Done / Delete' button."
            ),
            color=discord.Color.blue()
        )

        view = BreakBoardButtons(self.bot)
        try:
            message = await channel.send(embed=embed, view=view)
            self.save_message_id(message.id)
            print(f"SUCCESS(BreakBoard): Sent new BreakBoard message (ID: {message.id}) in channel {channel.name}.")
        except Exception as e:
            print(f"ERROR(BreakBoard): Failed to send initial embed message to channel {channel.name}: {e}")


async def setup(bot):
    await bot.add_cog(BreakBoard(bot))
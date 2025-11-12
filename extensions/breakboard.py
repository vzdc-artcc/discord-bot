import datetime
import discord
from bot import logger
from discord.ext import commands
import json
import os
from config import BREAK_BOARD_CHANNEL_ID, BREAK_BOARD_ROLE_MAP
import re

ROLE_SELECTOR_MESSAGE_ID_FILE = f"{os.getcwd()}/data/breakboard_selector_message_id.json"
MESSAGE_ID_FILE = f"{os.getcwd()}/data/notification_message_id.json"

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
        logger.info(f"Error during BreakRequestActions interaction for {item.custom_id}: {error}")
        if not interaction.response.is_done():
            await interaction.response.send_message("An error occurred with this action.", ephemeral=True)
        else:
            await interaction.followup.send("An error occurred after acknowledging this action.", ephemeral=True)

    @discord.ui.button(label="Claim Position", style=discord.ButtonStyle.success, custom_id="claim_break_position")
    async def claim_button(self, interaction: discord.Interaction, button: discord.ui.Button ):
        await interaction.response.defer()

        reliever_user = interaction.user
        requesting_user = interaction.guild.get_member(self.request_user_id)

        if requesting_user:
            await interaction.channel.send(
                f"🚨 {reliever_user.mention} has claimed the position for {requesting_user.mention}! "
                "Please coordinate directly.",
                view=NotificationDeleteView(reliever_user.id)
            )

            try:
                await interaction.message.delete()
            except Exception as e:
                logger.error("Error deleting break request message after claim: {e}")
        else:
            await interaction.channel.send(
                f"🚨 {reliever_user.mention} has claimed this break! The original requester is no longer in the server."
            )

        for item in self.children:
            item.disabled = True
        # await interaction.message.edit(view=self)

    @discord.ui.button(label="Done / Delete", style=discord.ButtonStyle.danger, custom_id="delete_break_request")
    async def delete_button(self, interaction: discord.Interaction, button: discord.ui.Button  ):

        await interaction.response.defer(ephemeral=True)

        is_requester = interaction.user.id == self.request_user_id

        can_delete = is_requester or interaction.user.guild_permissions.manage_messages

        if not can_delete:
            await interaction.followup.send("You are not authorized to delete this message.", ephemeral=True)
            return

        try:
            await interaction.message.delete()
            logger.info(f"Break request message deleted by {interaction.user.name}.")

        except discord.Forbidden:
            await interaction.followup.send("I don't have permission to delete this message.", ephemeral=True)
        except Exception as e:
            await interaction.followup.send(f"Failed to delete message: {e}", ephemeral=True)
            logger.info(f"Error deleting break request message: {e}")

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
            logger.info("Error: BreakBoard cog not found during modal submission.")
            await interaction.followup.send(
                "An internal error occurred: BreakBoard cog not found.", ephemeral=True
            )

    async def on_error(self, interaction: discord.Interaction, error: Exception):
        logger.info(f"Error submitting modal for {self.role_name}: {error}")
        if not interaction.response.is_done():
            await interaction.response.send_message("An error occurred while processing your request.", ephemeral=True)
        else:
            await interaction.followup.send("An error occurred after acknowledging your request.", ephemeral=True)


class BreakBoardButtons(discord.ui.View):
    def __init__(self, bot):
        super().__init__(timeout=None)
        self.bot = bot

    async def on_error(self, interaction: discord.Interaction, error: Exception, item: discord.ui.Item):
        logger.info(f"Error during button interaction for {item.custom_id}: {error}")
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

        if os.path.exists(MESSAGE_ID_FILE):
            with open(MESSAGE_ID_FILE, "r") as f:
                data = json.load(f)
                self.message_id = data.get("message_id")
                if data.get("channel_id") != self.channel_id:
                    logger.info("Warning: Notification channel ID mismatch in saved data. Resetting.")
                    self.message_id = None
        else:
            os.makedirs(os.path.dirname(MESSAGE_ID_FILE), exist_ok=True)

    def save_message_id(self, message_id: int):
        self.message_id = message_id
        with open(MESSAGE_ID_FILE, "w") as f:
            json.dump({"message_id": message_id, "channel_id": self.channel_id}, f)

    @commands.Cog.listener()
    async def on_ready(self):
        if not self.bot.is_ready():
            return

        logger.info("BreakBoard cog ready.")
        channel = self.bot.get_channel(self.channel_id)
        if not channel:
            logger.info(f"Error: BreakBoard channel with ID {self.channel_id} not found.")
            return

        if self.message_id:
            try:
                message = await channel.fetch_message(self.message_id)
                self.bot.add_view(BreakBoardButtons(self.bot), message_id=message.id)
                logger.info(f"Found existing BreakBoard message (ID: {self.message_id}). Re-attaching view.")
                return
            except discord.NotFound:
                logger.info("Previous BreakBoard message not found. Sending a new one.")
                self.message_id = None
            except discord.Forbidden:
                logger.info(f"Bot doesn't have permission to fetch message {self.message_id} in channel {self.channel_id}.")
                self.message_id = None

        await self.send_initial_embed_with_buttons(channel)

    async def send_notification(self, interaction: discord.Interaction, role_name: str, role_id: int,
                                wait_time: str = "no specific time"):

        with open(MESSAGE_ID_FILE, "r") as f:
            file = f.read()
        time = datetime.datetime.utcnow()
        logger.info(f"BREAKBOARD FILE: {file}, time: {time}")

        role = interaction.guild.get_role(role_id)
        if not role:
            await interaction.followup.send(
                f"Error: Role for '{role_name}' not found. Please contact an administrator.", ephemeral=True)
            return

        user_mention = interaction.user.mention
        message_to_send = (
            f"{role.mention} **{role_name} break request!** "
            f"Controller {user_mention} is requesting a relief for {role_name}. They can wait {wait_time}."
        )

        try:
            notification_channel = interaction.channel
            dynamic_view = BreakRequestActions(interaction.user.id)

            sent_message = await notification_channel.send(message_to_send, view=dynamic_view)
            dynamic_view.message = sent_message

        except Exception as e:
            await interaction.followup.send(f"Failed to send notification: {e}", ephemeral=True)
            logger.info(f"Failed to send notification for {role_name} (Role ID: {role_id}): {e}")

    async def send_initial_embed_with_buttons(self, channel: discord.TextChannel):
        embed = discord.Embed(
            title="Controller Break Notification System",
            description=(
                "Use the buttons below to request a break for specific positions.\n"
                "- The message will include a 'Claim' and 'Done / Delete' button."
                "- Press the 'Complete' button to delete the message when the shift change is complete."
            ),
            color=discord.Color.blue()
        )

        view = BreakBoardButtons(self.bot)
        message = await channel.send(embed=embed, view=view)
        self.save_message_id(message.id)
        logger.info(f"Sent new BreakBoard message (ID: {message.id}) in channel {channel.name}.")


class NotificationDeleteView(discord.ui.View):
    def __init__(self, allowed_user_id: int):
        super().__init__(timeout=300)
        self.allowed_user_id = allowed_user_id

    @discord.ui.button(label="Complete", style=discord.ButtonStyle.success, custom_id="delete_notification")
    async def delete_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        can_delete = (
                interaction.user.id == self.allowed_user_id or
                interaction.user.guild_permissions.manage_messages
        )
        if not can_delete:
            await interaction.response.send_message("You are not authorized to delete this message.", ephemeral=True)
            return
        try:
            await interaction.message.delete()
        except discord.Forbidden:
            await interaction.response.send_message("I don't have permission to delete this message.", ephemeral=True)
        except Exception as e:
            await interaction.response.send_message(f"Failed to delete message: {e}", ephemeral=True)
            

class RoleSelectionButtons(discord.ui.View):
    def __init__(self, bot):
        super().__init__(timeout=None)
        self.bot = bot

    async def on_error(self, interaction: discord.Interaction, error: Exception, item: discord.ui.Item):
        logger.info(f"Error during role selection interaction for {item.custom_id}: {error}")
        await interaction.response.send_message("An error occurred while processing your role request.", ephemeral=True)

    async def assign_or_remove_role(self, interaction: discord.Interaction, role_name_display: str, role_id: int):
        await interaction.response.defer(ephemeral=True)

        member = interaction.user
        role = interaction.guild.get_role(role_id)

        if not role:
            await interaction.followup.send(
                f"Error: Role '{role_name_display}' not found on the server. Please contact an administrator.",
                ephemeral=True
            )
            return

        if role in member.roles:
            try:
                await member.remove_roles(role)
                await interaction.followup.send(f"You have **left** the `{role_name_display}` notification group.", ephemeral=True)
            except discord.Forbidden:
                await interaction.followup.send("I don't have permissions to remove that role. Please check my permissions.", ephemeral=True)
            except Exception as e:
                await interaction.followup.send(f"An error occurred while removing the role: {e}", ephemeral=True)
        else:
            try:
                await member.add_roles(role)
                await interaction.followup.send(f"You have **joined** the `{role_name_display}` notification group.", ephemeral=True)
            except discord.Forbidden:
                await interaction.followup.send("I don't have permissions to add that role. Please check my permissions.", ephemeral=True)
            except Exception as e:
                await interaction.followup.send(f"An error occurred while adding the role: {e}", ephemeral=True)

    @discord.ui.button(label="Unrestricted GND", style=discord.ButtonStyle.secondary, custom_id="role_gnd_unrestricted")
    async def gnd_unrestricted_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "Unrestricted GND", BREAK_BOARD_ROLE_MAP["gnd_unrestricted"])

    @discord.ui.button(label="Tier 1 GND", style=discord.ButtonStyle.secondary, custom_id="role_gnd_tier1")
    async def gnd_tier1_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "Tier 1 GND", BREAK_BOARD_ROLE_MAP["gnd_tier1"])

    @discord.ui.button(label="Unrestricted TWR", style=discord.ButtonStyle.secondary, custom_id="role_twr_unrestricted")
    async def twr_unrestricted_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "Unrestricted TWR", BREAK_BOARD_ROLE_MAP["twr_unrestricted"])

    @discord.ui.button(label="Tier 1 TWR", style=discord.ButtonStyle.secondary, custom_id="role_twr_tier1")
    async def twr_tier1_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "Tier 1 TWR", BREAK_BOARD_ROLE_MAP["twr_tier1"])

    @discord.ui.button(label="Unrestricted APP", style=discord.ButtonStyle.secondary, custom_id="role_app_unrestricted")
    async def app_unrestricted_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "Unrestricted APP", BREAK_BOARD_ROLE_MAP["app_unrestricted"])

    @discord.ui.button(label="PCT", style=discord.ButtonStyle.secondary, custom_id="role_pct")
    async def pct_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "PCT", BREAK_BOARD_ROLE_MAP["pct"])

    @discord.ui.button(label="Center", style=discord.ButtonStyle.secondary, custom_id="role_center")
    async def center_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "Center", BREAK_BOARD_ROLE_MAP["center"])


class RoleSelector(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.message_id = None
        self.channel_id = BREAK_BOARD_CHANNEL_ID

        os.makedirs(os.path.dirname(ROLE_SELECTOR_MESSAGE_ID_FILE), exist_ok=True)

        if os.path.exists(ROLE_SELECTOR_MESSAGE_ID_FILE):
            with open(ROLE_SELECTOR_MESSAGE_ID_FILE, "r") as f:
                data = json.load(f)
                self.message_id = data.get("message_id")
                if data.get("channel_id") != self.channel_id:
                    logger.info("Warning: Role selector channel ID mismatch in saved data. Resetting.")
                    self.message_id = None

    def save_message_id(self, message_id: int):
        self.message_id = message_id
        with open(ROLE_SELECTOR_MESSAGE_ID_FILE, "w") as f:
            json.dump({"message_id": message_id, "channel_id": self.channel_id}, f)

    @commands.Cog.listener()
    async def on_ready(self):
        if not self.bot.is_ready():
            return

        logger.info("RoleSelector cog ready.")
        channel = self.bot.get_channel(self.channel_id)
        if not channel:
            logger.info(f"Error: Role Selector channel with ID {self.channel_id} not found.")
            return

        # Try to fetch existing message
        if self.message_id:
            try:
                message = await channel.fetch_message(self.message_id)
                self.bot.add_view(RoleSelectionButtons(self.bot), message_id=message.id)
                logger.info(f"Found existing role selector message (ID: {self.message_id}). Re-attaching view.")
                return # Message found, no need to send new
            except discord.NotFound:
                logger.info("Previous role selector message not found. Sending a new one.")
                self.message_id = None
            except discord.Forbidden:
                logger.info(f"Bot doesn't have permission to fetch message {self.message_id} in channel {self.channel_id}.")
                self.message_id = None
        await self.send_initial_embed_with_buttons(channel)

    async def send_initial_embed_with_buttons(self, channel: discord.TextChannel):
        embed = discord.Embed(
            title="🔔 Controller Notification Preferences 🔔",
            description=(
                "Click the buttons below to **opt in or out** of receiving notifications "
                "when controllers request a break for specific positions.\n\n"
                "• If you have the role, clicking the button will **remove** it.\n"
                "• If you don't have the role, clicking the button will **add** it."
            ),
            color=discord.Color.gold()
        )
        embed.set_footer(text="Your role preferences determine which break requests you see.")

        view = RoleSelectionButtons(self.bot)
        message = await channel.send(embed=embed, view=view)
        self.save_message_id(message.id)
        logger.info(f"Sent new role selector message (ID: {message.id}) in channel {channel.name}.")

async def setup(bot):
    await bot.add_cog(RoleSelector(bot))
    await bot.add_cog(BreakBoard(bot))
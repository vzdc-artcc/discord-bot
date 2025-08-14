import discord
from discord.ext import commands
import json
import os
from config import BREAK_BOARD_CHANNEL_ID, BREAK_BOARD_ROLE_MAP

ROLE_SELECTOR_MESSAGE_ID_FILE = "data/breakboard_selector_message_id.json"

class RoleSelectionButtons(discord.ui.View):
    def __init__(self, bot):
        super().__init__(timeout=None)
        self.bot = bot

    async def on_error(self, interaction: discord.Interaction, error: Exception, item: discord.ui.Item):
        print(f"Error during role selection interaction for {item.custom_id}: {error}")
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
                    print("Warning: Role selector channel ID mismatch in saved data. Resetting.")
                    self.message_id = None

    def save_message_id(self, message_id: int):
        self.message_id = message_id
        with open(ROLE_SELECTOR_MESSAGE_ID_FILE, "w") as f:
            json.dump({"message_id": message_id, "channel_id": self.channel_id}, f)

    @commands.Cog.listener()
    async def on_ready(self):
        if not self.bot.is_ready():
            return

        print("RoleSelector cog ready.")
        channel = self.bot.get_channel(self.channel_id)
        if not channel:
            print(f"Error: Role Selector channel with ID {self.channel_id} not found.")
            return

        # Try to fetch existing message
        if self.message_id:
            try:
                message = await channel.fetch_message(self.message_id)
                self.bot.add_view(RoleSelectionButtons(self.bot), message_id=message.id)
                print(f"Found existing role selector message (ID: {self.message_id}). Re-attaching view.")
                return # Message found, no need to send new
            except discord.NotFound:
                print("Previous role selector message not found. Sending a new one.")
                self.message_id = None
            except discord.Forbidden:
                print(f"Bot doesn't have permission to fetch message {self.message_id} in channel {self.channel_id}.")
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
        print(f"Sent new role selector message (ID: {message.id}) in channel {channel.name}.")

async def setup(bot):
    await bot.add_cog(RoleSelector(bot))
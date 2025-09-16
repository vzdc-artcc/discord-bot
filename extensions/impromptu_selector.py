import discord
from discord.ext import commands
import json
import os
from config import IMPROMPTU_CHANNEL_ID, IMPROMPTU_ROLE_MAP

ROLE_SELECTOR_MESSAGE_ID_FILE = f"{os.getcwd()}/data/impromptu_selector_message_id.json"

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

    @discord.ui.button(label="Ground", style=discord.ButtonStyle.secondary, custom_id="role_impromptu_gnd")
    async def impromptu_gnd_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.remove_existing_roles(interaction, IMPROMPTU_ROLE_MAP["impromptu_gnd"])
        await self.assign_or_remove_role(interaction, "Impromptu Ground", IMPROMPTU_ROLE_MAP["impromptu_gnd"])

    @discord.ui.button(label="Tower", style=discord.ButtonStyle.secondary, custom_id="role_impromptu_twr")
    async def impromptu_twr_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.remove_existing_roles(interaction, IMPROMPTU_ROLE_MAP["impromptu_twr"])
        await self.assign_or_remove_role(interaction, "Impromptu Tower", IMPROMPTU_ROLE_MAP["impromptu_twr"])

    @discord.ui.button(label="Approach", style=discord.ButtonStyle.secondary, custom_id="role_impromptu_app")
    async def impromptu_app_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.remove_existing_roles(interaction, IMPROMPTU_ROLE_MAP["impromptu_app"])
        await self.assign_or_remove_role(interaction, "Impromptu Approach", IMPROMPTU_ROLE_MAP["impromptu_app"])

    @discord.ui.button(label="Center", style=discord.ButtonStyle.secondary, custom_id="role_impromptu_ctr")
    async def impromptu_ctr_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.remove_existing_roles(interaction, IMPROMPTU_ROLE_MAP["impromptu_ctr"])
        await self.assign_or_remove_role(interaction, "Impromptu Center", IMPROMPTU_ROLE_MAP["impromptu_ctr"])

    async def remove_existing_roles(self, interaction: discord.Interaction, exclude_role_id):
        member = interaction.user
        roles_to_remove = [interaction.guild.get_role(rid) for rid in IMPROMPTU_ROLE_MAP.values() if interaction.guild.get_role(rid) in member.roles and rid != exclude_role_id]
        if roles_to_remove:
            try:
                await member.remove_roles(*roles_to_remove)
                await interaction.response.send_message("You have **left** all impromptu notification groups.", ephemeral=True)
            except discord.Forbidden:
                await interaction.response.send_message("I don't have permissions to remove those roles. Please check my permissions.", ephemeral=True)
            except Exception as e:
                await interaction.response.send_message(f"An error occurred while removing the roles: {e}", ephemeral=True)

class ImpromptuSelector(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.message_id = None
        self.channel_id = IMPROMPTU_CHANNEL_ID

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

        if self.message_id:
            try:
                message = await channel.fetch_message(self.message_id)
                self.bot.add_view(RoleSelectionButtons(self.bot), message_id=message.id)
                print(f"Found existing role selector message (ID: {self.message_id}). Re-attaching view.")
                return
            except discord.NotFound:
                print("Previous role selector message not found. Sending a new one.")
                self.message_id = None
            except discord.Forbidden:
                print(f"Bot doesn't have permission to fetch message {self.message_id} in channel {self.channel_id}.")
                self.message_id = None
        await self.send_initial_embed_with_buttons(channel)

    async def send_initial_embed_with_buttons(self, channel: discord.TextChannel):
        embed = discord.Embed(
            title="Impromptu Selector",
            description=(
                "As a perk of being a ZDC home controller, you may join a role that our training staff can use to alert you of an available impromptu session.  These sessions will usually be the same day.  Keep reading for instructions on how to do this.\n"
                "This will add you to a role that the training staff can ping in this channel.  If you get an alert that there is an open session, you may PM the instructor who posted.  These sessions are first come – first serve.  The instructor will delete the message or react to it to indicate it has been taken.\n\n"
                "Click the buttons below to **opt in or out** of receiving notifications for **the training that you are seeking**\n "
                "• If you have the role, clicking the button will **remove** it.\n"
                "• If you don't have the role, clicking the button will **add** it."
            ),
            color=discord.Color.gold()
        )

        view = RoleSelectionButtons(self.bot)
        message = await channel.send(embed=embed, view=view)
        self.save_message_id(message.id)
        print(f"Sent new role selector message (ID: {message.id}) in channel {channel.name}.")

async def setup(bot):
    await bot.add_cog(ImpromptuSelector(bot))
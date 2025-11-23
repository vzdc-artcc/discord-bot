import logging

import discord
from discord.ext import commands
import json
import os
from bot import logger
import config as cfg

ROLE_SELECTOR_MESSAGE_ID_FILE = f"{os.getcwd()}/data/impromptu_selector_message_id.json"

def _role_selector_file_for_guild(guild_id: int):
    return f"{os.getcwd()}/data/impromptu_selector_message_id_{guild_id}.json"

class RoleSelectionButtons(discord.ui.View):
    def __init__(self, bot):
        super().__init__(timeout=None)
        self.bot = bot

    async def on_error(self, interaction: discord.Interaction, error: Exception, item: discord.ui.Item):
        logger.exception(f"Error during role selection interaction for {getattr(item, 'custom_id', repr(item))}: {error}")
        # Use followup if we already responded; otherwise send a response.
        if interaction.response.is_done():
            await interaction.followup.send("An error occurred while processing your role request.", ephemeral=True)
        else:
            await interaction.response.send_message("An error occurred while processing your role request.", ephemeral=True)

    async def assign_or_remove_role(self, interaction: discord.Interaction, role_name_display: str, role_id: int):
        # Defer only if we have not already responded to this interaction.
        if not interaction.response.is_done():
            await interaction.response.defer(ephemeral=True)

        member = interaction.user
        guild = interaction.guild
        role = guild.get_role(role_id)

        if not role:
            # If a response has already been done (or we deferred), use followup, otherwise send a normal response.
            if interaction.response.is_done():
                await interaction.followup.send(
                    f"Error: Role '{role_name_display}' not found on the server. Please contact an administrator.",
                    ephemeral=True
                )
            else:
                await interaction.response.send_message(
                    f"Error: Role '{role_name_display}' not found on the server. Please contact an administrator.",
                    ephemeral=True
                )
            return

        # If the user already has the role, just remove it (toggle off).
        if role in member.roles:
            try:
                await member.remove_roles(role)
                if interaction.response.is_done():
                    await interaction.followup.send(f"You have **left** the `{role_name_display}` notification group.", ephemeral=True)
                else:
                    await interaction.response.send_message(f"You have **left** the `{role_name_display}` notification group.", ephemeral=True)
            except discord.Forbidden:
                if interaction.response.is_done():
                    await interaction.followup.send("I don't have permissions to remove that role. Please check my permissions.", ephemeral=True)
                else:
                    await interaction.response.send_message("I don't have permissions to remove that role. Please check my permissions.", ephemeral=True)
            except Exception as e:
                if interaction.response.is_done():
                    await interaction.followup.send(f"An error occurred while removing the role: {e}", ephemeral=True)
                else:
                    await interaction.response.send_message(f"An error occurred while removing the role: {e}", ephemeral=True)
            return

        # We're adding the role. Before attempting the add, verify the bot can actually add the target role.
        bot_member = guild.me
        # Check basic permission
        if not bot_member.guild_permissions.manage_roles:
            if interaction.response.is_done():
                await interaction.followup.send("I don't have the Manage Roles permission. I cannot add roles.", ephemeral=True)
            else:
                await interaction.response.send_message("I don't have the Manage Roles permission. I cannot add roles.", ephemeral=True)
            return

        # Check role hierarchy: bot's top role must be higher than the role to assign
        if bot_member.top_role.position <= role.position:
            if interaction.response.is_done():
                await interaction.followup.send("I can't assign that role because it is higher than (or equal to) my highest role. Please contact an administrator.", ephemeral=True)
            else:
                await interaction.response.send_message("I can't assign that role because it is higher than (or equal to) my highest role. Please contact an administrator.", ephemeral=True)
            return

        # Try to add the role first. Only after a successful add do we remove other impromptu roles.
        try:
            await member.add_roles(role)
            # Now remove other impromptu roles (if any) excluding the one we just added.
            await self.remove_existing_roles(interaction, role_id)

            if interaction.response.is_done():
                await interaction.followup.send(f"You have **joined** the `{role_name_display}` notification group.", ephemeral=True)
            else:
                await interaction.response.send_message(f"You have **joined** the `{role_name_display}` notification group.", ephemeral=True)
        except discord.Forbidden:
            if interaction.response.is_done():
                await interaction.followup.send("I don't have permissions to add that role. Please check my permissions.", ephemeral=True)
            else:
                await interaction.response.send_message("I don't have permissions to add that role. Please check my permissions.", ephemeral=True)
        except Exception as e:
            if interaction.response.is_done():
                await interaction.followup.send(f"An error occurred while adding the role: {e}", ephemeral=True)
            else:
                await interaction.response.send_message(f"An error occurred while adding the role: {e}", ephemeral=True)

    @discord.ui.button(label="Ground", style=discord.ButtonStyle.secondary, custom_id="role_impromptu_gnd")
    async def impromptu_gnd_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "Impromptu Ground", cfg.get_role_for_guild(interaction.guild.id, "impromptu_gnd"))

    @discord.ui.button(label="Tower", style=discord.ButtonStyle.secondary, custom_id="role_impromptu_twr")
    async def impromptu_twr_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "Impromptu Tower", cfg.get_role_for_guild(interaction.guild.id, "impromptu_twr"))

    @discord.ui.button(label="Approach", style=discord.ButtonStyle.secondary, custom_id="role_impromptu_app")
    async def impromptu_app_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "Impromptu Approach", cfg.get_role_for_guild(interaction.guild.id, "impromptu_app"))

    @discord.ui.button(label="Center", style=discord.ButtonStyle.secondary, custom_id="role_impromptu_ctr")
    async def impromptu_ctr_role_button(self, interaction: discord.Interaction, button: discord.ui.Button):
        await self.assign_or_remove_role(interaction, "Impromptu Center", cfg.get_role_for_guild(interaction.guild.id, "impromptu_ctr"))

    async def remove_existing_roles(self, interaction: discord.Interaction, exclude_role_id):
        member = interaction.user
        guild = interaction.guild
        # Build list of roles (actual Role objects) to remove, excluding the role we are about to toggle.
        roles_to_remove = [guild.get_role(rid) for rid in [cfg.get_role_for_guild(guild.id, k) for k in ("impromptu_ctr","impromptu_app","impromptu_twr","impromptu_gnd")] if rid and guild.get_role(rid) in member.roles and rid != exclude_role_id]
        if roles_to_remove:
            try:
                await member.remove_roles(*roles_to_remove)
                # Do not send a primary response here — leave messaging to the main handler.
            except discord.Forbidden:
                # If removal fails due to permissions, log and inform the user if we've already responded.
                logger.exception("I don't have permissions to remove those roles. Please check bot permissions.")
                if interaction.response.is_done():
                    await interaction.followup.send("I don't have permissions to remove those roles. Please check my permissions.", ephemeral=True)
            except Exception as e:
                logger.exception(f"An error occurred while removing the roles: {e}")
                if interaction.response.is_done():
                    await interaction.followup.send(f"An error occurred while removing the roles: {e}", ephemeral=True)

class ImpromptuSelector(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.message_id = None
        self.channel_id = None

        # ensure data dir
        os.makedirs(os.path.join(os.getcwd(), "data"), exist_ok=True)
        # we store message ids per guild now
        self.message_id = None

    def save_message_id(self, message_id: int, channel_id: int):
        self.message_id = message_id
        try:
            guild_id = channel_id and getattr(self.bot.get_channel(channel_id).guild, "id", None)
            if guild_id:
                path = _role_selector_file_for_guild(guild_id)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w") as f:
                    json.dump({"message_id": message_id, "channel_id": channel_id}, f)
        except Exception:
            # best-effort persistence
            pass

    @commands.Cog.listener()
    async def on_ready(self):
        if not self.bot.is_ready():
            return

        logger.info("RoleSelector cog ready.")
        for guild in self.bot.guilds:
            guild_cfg = cfg.get_guild_config(guild.id)
            channel_id = guild_cfg.get_channel("impromptu_channel_id")
            if not channel_id:
                logger.info(f"No role selector channel configured for guild {guild.id} ({guild.name}), skipping.")
                continue

            channel = self.bot.get_channel(channel_id) or await self.bot.fetch_channel(channel_id)
            if not channel:
                logging.error(f"Role Selector channel with ID {channel_id} not found.")
                continue

            # check per-guild saved message id
            msg_file = _role_selector_file_for_guild(guild.id)
            saved_message_id = None
            if os.path.exists(msg_file):
                try:
                    with open(msg_file, "r") as f:
                        data = json.load(f)
                        saved_message_id = data.get("message_id")
                except Exception:
                    saved_message_id = None

            if saved_message_id:
                try:
                    message = await channel.fetch_message(saved_message_id)
                    self.bot.add_view(RoleSelectionButtons(self.bot), message_id=message.id)
                    logger.info(f"Found existing role selector message (ID: {saved_message_id}) for guild {guild.id}. Re-attaching view.")
                    return
                except discord.NotFound:
                    logger.info("Previous role selector message not found. Sending a new one.")
                except discord.Forbidden:
                    logger.exception(f"Bot doesn't have permission to fetch message {saved_message_id} in channel {channel_id}.")
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
        try:
            self.save_message_id(message.id, channel.id)
        except Exception:
            pass
        logger.info(f"Sent new role selector message (ID: {message.id}) in channel {channel.name}.")


async def setup(bot):
    await bot.add_cog(ImpromptuSelector(bot))
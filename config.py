import os
from dotenv import load_dotenv

load_dotenv()

# Gerneral
DISCORD_TOKEN = os.getenv("DISCORD_TOKEN")

# VATUSA
VATUSA_API_KEY = os.getenv("VATUSA_API_KEY")
VATUSA_API_URL = os.getenv("VATUSA_API_URL")

# Channels

STAFFUP_CHANNEL = int(os.getenv("STAFFUP_CHANNEL"))


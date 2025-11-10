async def get_real_name(self, cid:str, VATUSA_API_KEY, VATUSA_API_URL) -> str:
    """Get the real name of a controller from VATUSA by their CID."""
    import aiohttp

    url = f"{VATUSA_API_URL}/user/{cid}"
    headers = {"Authorization": f"Token {VATUSA_API_KEY}"}

    try:
        async with aiohttp.ClientSession() as session:
            async with session.get(url, headers=headers) as response:
                if response.status == 200:
                    data = await response.json()
                    return data.get("name", "Unknown")
                else:
                    self.logger.error(f"Failed to fetch real name for CID {cid}. Status code: {response.status}")
                    return "Unknown"
    except Exception as e:
        self.logger.error(f"Exception occurred while fetching real name for CID {cid}: {e}")
        return "Unknown"
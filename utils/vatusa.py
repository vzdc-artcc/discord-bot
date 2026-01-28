from config import VATUSA_API_URL

def get_real_name(cid, VATUSA_API_URL) -> str:
    import requests

    url = f"{VATUSA_API_URL}/user/{cid}"
    res = requests.get(url)
    if res.status_code != 200:
        return "Unknown User"

    payload = res.json()
    user = payload.get("data")

    fname = (user.get("fname")).strip()
    lname = (user.get("lname")).strip()

    full = f"{fname} {lname}".strip()
    return full


def main():
    print(get_real_name( 1652726, VATUSA_API_URL))


if __name__ == "__main__":
    main()
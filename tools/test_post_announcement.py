"""Simple script to POST a test announcement to the local API server.

Usage (fish shell):
  python tools/test_post_announcement.py --api-key=MYKEY

It uses only the standard library so you don't need extra packages.
"""
import argparse
import json
import sys
from urllib import request as urlrequest


def post_announcement(api_key: str, base_url: str = "http://localhost:6000"):
    url = f"{base_url}/announcements"
    payload = {
        "message_type": "event-posting",
        "title": "Test Event from Script",
        "body": "This is a test message posted by tools/test_post_announcement.py",
        "author_name": "Test Runner",
        "banner_url": "https://example.com/test-banner.jpg"
    }

    data = json.dumps(payload).encode("utf-8")
    req = urlrequest.Request(url, data=data, method="POST")
    req.add_header("Content-Type", "application/json")
    req.add_header("X-API-Key", api_key)

    try:
        with urlrequest.urlopen(req) as resp:
            resp_body = resp.read().decode("utf-8")
            print(f"Status: {resp.status}")
            print("Response:")
            print(resp_body)
    except Exception as e:
        print(f"Request failed: {e}")
        return 1
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--api-key", required=True, help="API key to use (X-API-Key header)")
    parser.add_argument("--base-url", default="http://localhost:6000", help="Base URL of the API server")
    args = parser.parse_args()
    sys.exit(post_announcement(args.api_key, args.base_url))


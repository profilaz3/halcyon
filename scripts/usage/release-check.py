#!/usr/bin/env python3
"""Fail a release before building if the activity collector is not configured."""
import argparse
import json
from pathlib import Path
import re
import urllib.parse
import urllib.request


def validate_endpoint(value):
    value = value.strip()
    url = urllib.parse.urlsplit(value)
    if (url.scheme != "https" or url.hostname != "script.google.com"
            or url.port not in (None, 443) or url.username or url.password
            or url.query or url.fragment
            or not re.fullmatch(r"/macros/s/[A-Za-z0-9_-]+/exec", url.path)):
        raise ValueError("Configure assets/usage-endpoint.txt with the deployed Google web app URL")
    return value


class GoogleRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        url = urllib.parse.urlsplit(newurl)
        if (url.scheme != "https" or url.hostname not in
                ("script.google.com", "script.googleusercontent.com")
                or url.port not in (None, 443) or url.username or url.password):
            raise ValueError("Collector redirected outside Google")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-live", action="store_true", help="Check the public health response; no activity report is sent")
    args = parser.parse_args()
    path = Path(__file__).resolve().parents[2] / "assets/usage-endpoint.txt"
    try:
        endpoint = validate_endpoint(path.read_text())
        if args.check_live:
            opener = urllib.request.build_opener(GoogleRedirects())
            with opener.open(endpoint, timeout=20) as response:
                body = response.read(1025)
                if (response.status != 200 or len(body) > 1024
                        or json.loads(body) != {"service": "halcyon-activity", "schema": 5}):
                    raise ValueError("Collector must support package and activity metadata (schema 5); deploy and verify the collector before releasing")
    except Exception as error:
        raise SystemExit(f"Release blocked: {error}") from None
    print("Activity collector endpoint verified" + (" (live health check)" if args.check_live else " (configuration only)"))


if __name__ == "__main__":
    main()

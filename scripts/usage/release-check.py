#!/usr/bin/env python3
"""Fail a release before building if the activity collector is not configured."""
import argparse
import json
from pathlib import Path
import re
import urllib.parse
import urllib.request


COLLECTOR_HOST = "halcyon-reports.profilaz3.workers.dev"


def validate_endpoint(value):
    value = value.strip()
    url = urllib.parse.urlsplit(value)
    if (url.scheme != "https" or url.hostname != COLLECTOR_HOST
            or url.port not in (None, 443) or url.username or url.password
            or url.query or url.fragment or url.path != "/v1/report"):
        raise ValueError(f"Configure assets/usage-endpoint.txt with https://{COLLECTOR_HOST}/v1/report")
    return value


class NoRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("Collector must answer directly, without a redirect")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-live", action="store_true", help="Check the public health response; no activity report is sent")
    args = parser.parse_args()
    path = Path(__file__).resolve().parents[2] / "assets/usage-endpoint.txt"
    try:
        endpoint = validate_endpoint(path.read_text())
        if args.check_live:
            opener = urllib.request.build_opener(NoRedirects())
            # Cloudflare rejects the default Python-urllib agent (error 1010).
            request = urllib.request.Request(endpoint, headers={"User-Agent": "halcyon-release-check"})
            with opener.open(request, timeout=20) as response:
                body = response.read(1025)
                if (response.status != 200 or len(body) > 1024
                        or json.loads(body) != {"service": "halcyon-activity", "schema": 6}):
                    raise ValueError("Collector must support the feature snapshot (schema 6); deploy and verify the collector before releasing")
    except Exception as error:
        raise SystemExit(f"Release blocked: {error}") from None
    print("Activity collector endpoint verified" + (" (live health check)" if args.check_live else " (configuration only)"))


if __name__ == "__main__":
    main()

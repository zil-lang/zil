#!/usr/bin/env python3
"""Bounded, read-only previous-run diagnostics; never changes gate receipts."""
import argparse
import datetime
import json
import os
from pathlib import Path
import signal
import time
import urllib.error
import urllib.parse
import urllib.request

REPOSITORY = "zil-lang/zil"
RUN_ID = 40870798002
TRANSPORT_SHA = "728b61fdb6782cb2915a881c18e3b4384b02552f"
CHECK_IDS = (123437021316, 123441986047)
NAME = "verify-and-publish"
PREFIX = "https://api.github.com/repos/" + REPOSITORY + "/check-runs/"
MAX_RESPONSE = 2 * 1024 * 1024
MAX_PAGES = 3
REQUEST_TIMEOUT = 8
TOTAL_TIMEOUT = 45


class DiagnosticRefusal(Exception):
    pass


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise urllib.error.HTTPError(req.full_url, code, "redirect refused", None, None)


def public_metadata(data):
    return {key: data[key] for key in ("id", "name", "head_sha", "details_url", "html_url",
            "status", "conclusion", "started_at", "completed_at", "output") if key in data}


def validate_metadata(data, check_id):
    if not isinstance(data, dict) or data.get("id") != check_id or data.get("name") != NAME:
        raise DiagnosticRefusal("metadata identity")
    if data.get("head_sha") != TRANSPORT_SHA:
        raise DiagnosticRefusal("transport identity")
    url = urllib.parse.urlsplit(data.get("details_url") or "")
    prefix = "/" + REPOSITORY + "/actions/runs/" + str(RUN_ID)
    if url.scheme != "https" or url.netloc != "github.com" or not (
            url.path == prefix or url.path.startswith(prefix + "/")):
        raise DiagnosticRefusal("run identity")


class Client:
    def __init__(self, token):
        self.token = token
        self.deadline = time.monotonic() + TOTAL_TIMEOUT
        self.opener = urllib.request.build_opener(NoRedirect())

    def get(self, url, requests):
        if not self.token or not url.startswith(PREFIX):
            raise DiagnosticRefusal("token or endpoint unavailable")
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError()
        request = urllib.request.Request(url, method="GET", headers={
            "Accept": "application/vnd.github+json", "Authorization": "Bearer " + self.token,
            "X-GitHub-Api-Version": "2026-03-10", "User-Agent": "zil-readonly-prior-job-diagnostics"})
        record = {"url": url, "http_status": None}
        requests.append(record)
        try:
            with self.opener.open(request, timeout=min(REQUEST_TIMEOUT, remaining)) as response:
                record["http_status"] = response.status
                if response.status != 200:
                    raise DiagnosticRefusal("unexpected status")
                body = response.read(MAX_RESPONSE + 1)
                if len(body) > MAX_RESPONSE:
                    raise DiagnosticRefusal("response limit")
                return json.loads(body)
        except urllib.error.HTTPError as error:
            record["http_status"] = error.code
            raise


def collect(token, client=None):
    client = client or Client(token)
    report = {"schema": 1, "repository": REPOSITORY, "previous_run_id": RUN_ID,
              "previous_transport_sha": TRANSPORT_SHA,
              "collected_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "diagnostic_only": True, "gate_effect": "None", "records": []}
    for check_id in CHECK_IDS:
        item = {"requested_check_run_id": check_id, "status": "Unavailable",
                "requests": [], "annotations": []}
        report["records"].append(item)
        try:
            data = client.get(PREFIX + str(check_id), item["requests"])
            if isinstance(data, dict):
                item["metadata"] = public_metadata(data)
            validate_metadata(data, check_id)
            item["identity_verified"] = True
            item["truncated"] = False
            for page in range(1, MAX_PAGES + 1):
                values = client.get(PREFIX + str(check_id) + "/annotations?per_page=100&page=" + str(page),
                                    item["requests"])
                if not isinstance(values, list) or len(values) > 100:
                    raise DiagnosticRefusal("invalid annotations response")
                item["annotations"].extend(values)
                if len(values) < 100:
                    break
                if page == MAX_PAGES:
                    item["truncated"] = True
            item["status"] = "Partial" if item["truncated"] else "Collected"
        except Exception as error:
            item["error_type"] = type(error).__name__
            item["status"] = "Unavailable"
    report["status"] = "Collected" if all(row["status"] == "Collected" for row in report["records"]) else "PartialOrUnavailable"
    # Even an unexpected server body must not put the supplied token in the artifact.
    encoded = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if token:
        encoded = encoded.replace(token, "[redacted]")
    return encoded


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    token = os.environ.pop("GITHUB_TOKEN", "")
    def timeout(signum, frame):
        raise TimeoutError()
    old_handler = signal.signal(signal.SIGALRM, timeout)
    signal.setitimer(signal.ITIMER_REAL, TOTAL_TIMEOUT)
    try:
        content = collect(token)
    except Exception as error:
        content = json.dumps({"schema": 1, "repository": REPOSITORY, "previous_run_id": RUN_ID,
            "previous_transport_sha": TRANSPORT_SHA, "diagnostic_only": True, "gate_effect": "None",
            "status": "Unavailable", "error_type": type(error).__name__}) + "\n"
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, old_handler)
    output = Path(args.out)
    try:
        output.parent.mkdir(parents=True, exist_ok=True)
        temporary = output.with_suffix(output.suffix + ".tmp")
        temporary.write_text(content, encoding="utf-8")
        temporary.replace(output)
    except OSError as error:
        # No path, request object, exception message, token, or headers are printed.
        print("Previous-job diagnostic write unavailable: " + type(error).__name__)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

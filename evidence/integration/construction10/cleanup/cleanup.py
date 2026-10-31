#!/usr/bin/env python3
"""Validate an approved ZIL cleanup manifest; delete only with --execute.

Requires normal, already configured Git and gh authentication. The script never
reads credentials, changes authentication, or writes main. Default mode performs
read-only remote checks and emits a receipt. No cleanup manifest is supplied here.

Manifest schema:
{
  "schema_version": 1,
  "repository": "zil-lang/zil",
  "accepted_head": "<exact 40-character lowercase SHA of accepted main CI>",
  "main_ci": {"run_id": 123, "run_attempt": 1},
  "branches": [
    {"ref": "refs/heads/impl/225-product-store-capture",
     "expected_sha": "<exact 40-character lowercase SHA>"},
    {"ref": "refs/heads/impl/226-shared-product-capture",
     "expected_sha": "<exact 40-character lowercase SHA>"},
    {"ref": "refs/heads/integration/construction10",
     "expected_sha": "<exact 40-character lowercase SHA>"},
    {"ref": "refs/heads/review/construction10-231",
     "expected_sha": "<exact 40-character lowercase SHA>"}
  ]
}

One to four unique allowlisted branches may be supplied. Every expected tip and
accepted_head must be ancestors of freshly fetched main. The named CI must
be the successful push/main verify-main workflow at accepted_head. Atomic push
and explicit per-ref force-with-lease prevent deleting a concurrently moved tip.
A failed or uncertain push is recorded and is never automatically retried.

Usage:
  python cleanup_merged_branches_v2.py --manifest approved.json --receipt dry-run.json
  python cleanup_merged_branches_v2.py --manifest approved.json --receipt execute.json --execute
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from typing import Any


REPOSITORY = "zil-lang/zil"
REMOTE_URL = "https://github.com/zil-lang/zil.git"
MAIN_REF = "refs/heads/main"
MAIN_WORKFLOW = ".github/workflows/verify-main.yml"
ALLOWED_REFS = frozenset(
    {
        "refs/heads/impl/225-product-store-capture",
        "refs/heads/impl/226-shared-product-capture",
        "refs/heads/integration/construction10",
        "refs/heads/review/construction10-231",
    }
)
SHA_RE = re.compile(r"[0-9a-f]{40}\Z")


class Refusal(RuntimeError):
    pass


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise Refusal(message)


def exact_keys(value: Any, names: set[str], label: str) -> None:
    require(isinstance(value, dict), f"{label} must be an object")
    require(set(value) == names, f"{label} keys must be exactly {sorted(names)}")


def valid_sha(value: Any) -> bool:
    return isinstance(value, str) and SHA_RE.fullmatch(value) is not None and value != "0" * 40


def read_manifest(path: Path) -> tuple[dict[str, Any], str]:
    raw = path.read_bytes()
    require(len(raw) <= 16384, "manifest exceeds 16 KiB")
    manifest = json.loads(raw)
    exact_keys(
        manifest,
        {"schema_version", "repository", "accepted_head", "main_ci", "branches"},
        "manifest",
    )
    require(type(manifest["schema_version"]) is int and manifest["schema_version"] == 1,
            "unsupported manifest schema")
    require(manifest["repository"] == REPOSITORY, "repository is not zil-lang/zil")
    require(valid_sha(manifest["accepted_head"]), "accepted_head must be a nonzero exact SHA")
    exact_keys(manifest["main_ci"], {"run_id", "run_attempt"}, "main_ci")
    for field in ("run_id", "run_attempt"):
        require(type(manifest["main_ci"][field]) is int and manifest["main_ci"][field] > 0,
                f"main_ci.{field} must be a positive integer")
    branches = manifest["branches"]
    require(isinstance(branches, list) and 1 <= len(branches) <= 4,
            "branches must contain one to four entries")
    seen: set[str] = set()
    for branch in branches:
        exact_keys(branch, {"ref", "expected_sha"}, "branch")
        ref = branch["ref"]
        require(isinstance(ref, str) and ref in ALLOWED_REFS,
                "branch ref is not in the fixed four-ref allowlist")
        require(ref not in seen, f"duplicate branch ref: {ref}")
        require(valid_sha(branch["expected_sha"]), f"{ref} must have an exact expected_sha")
        seen.add(ref)
    return manifest, hashlib.sha256(raw).hexdigest()


class Recorder:
    def __init__(self, path: Path, execute: bool, manifest_path: Path):
        self.path = path
        self.value: dict[str, Any] = {
            "schema_version": 1,
            "repository": REPOSITORY,
            "mode": "execute" if execute else "dry-run",
            "manifest_path": str(manifest_path),
            "started_at": utc_now(),
            "status": "Started",
            "mutation_attempted": False,
            "steps": [],
        }

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        temporary = self.path.with_name(self.path.name + ".tmp")
        temporary.write_text(json.dumps(self.value, indent=2) + "\n", encoding="utf-8")
        os.replace(temporary, self.path)

    def check(self, name: str, inputs: Any, result: Any) -> None:
        self.value["steps"].append(
            {"step": name, "time": utc_now(), "inputs": inputs, "result": result}
        )
        self.save()

    def command(self, name: str, argv: list[str], *, json_output: bool = False,
                allowed_codes: tuple[int, ...] = (0,), timeout: int = 180) -> str:
        entry: dict[str, Any] = {"step": name, "argv": argv, "started_at": utc_now()}
        self.value["steps"].append(entry)
        self.save()
        # Keep configured credentials, proxy and TLS settings. Remove only Git
        # repository selectors so all local Git writes stay in the temporary repo.
        environment = dict(os.environ)
        for key in (
            "GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_NAMESPACE",
        ):
            environment.pop(key, None)
        environment["GIT_TERMINAL_PROMPT"] = "0"
        try:
            result = subprocess.run(
                argv, stdin=subprocess.DEVNULL, capture_output=True, text=True,
                timeout=timeout, env=environment, check=False,
            )
            entry.update(
                completed_at=utc_now(), returncode=result.returncode,
                stdout_sha256=hashlib.sha256(result.stdout.encode()).hexdigest(),
                stderr=result.stderr[-16000:],
            )
            if not json_output:
                entry["stdout"] = result.stdout[-16000:]
            self.save()
            require(result.returncode in allowed_codes,
                    f"{name} failed with exit {result.returncode}; see receipt")
            return result.stdout
        except (OSError, subprocess.TimeoutExpired) as error:
            entry.update(completed_at=utc_now(), error=str(error))
            self.save()
            raise Refusal(f"{name} failed or timed out; no automatic retry") from error

    def api(self, name: str, endpoint: str) -> dict[str, Any]:
        value = json.loads(self.command(
            name,
            ["gh", "api", "--hostname", "github.com", "--method", "GET", endpoint],
            json_output=True,
        ))
        require(isinstance(value, dict), f"{name} did not return a JSON object")
        return value


def check_ci(recorder: Recorder, manifest: dict[str, Any], label: str) -> None:
    ci = manifest["main_ci"]
    run = recorder.api(label, f"repos/{REPOSITORY}/actions/runs/{ci['run_id']}")
    observed = {
        field: run.get(field)
        for field in (
            "id", "run_attempt", "status", "conclusion", "event",
            "head_branch", "head_sha", "path", "workflow_id", "html_url",
        )
    }
    observed["repository"] = (run.get("repository") or {}).get("full_name")
    observed["head_repository"] = (run.get("head_repository") or {}).get("full_name")
    recorder.check(label + "-identity", ci, observed)
    require(observed["repository"] == REPOSITORY, "CI repository mismatch")
    require(observed["head_repository"] == REPOSITORY, "CI head repository mismatch")
    require(observed["id"] == ci["run_id"], "CI run ID mismatch")
    require(observed["run_attempt"] == ci["run_attempt"], "CI attempt has changed")
    require(observed["path"] == MAIN_WORKFLOW, "CI is not the designated main workflow")
    require(observed["event"] == "push" and observed["head_branch"] == "main",
            "CI must be a push run on main")
    require(observed["status"] == "completed" and observed["conclusion"] == "success",
            "main CI is not completed successfully")
    require(observed["head_sha"] == manifest["accepted_head"], "CI accepted_head mismatch")


def remote_refs(recorder: Recorder, refs: list[str], label: str) -> dict[str, str]:
    output = recorder.command(label, ["git", "ls-remote", "--refs", REMOTE_URL, *refs])
    observed: dict[str, str] = {}
    for line in output.splitlines():
        fields = line.split()
        require(len(fields) == 2 and valid_sha(fields[0]), "malformed ls-remote response")
        sha, ref = fields
        require(ref in refs and ref not in observed, "unexpected or duplicate remote ref")
        observed[ref] = sha
    return observed


def perform(recorder: Recorder, manifest: dict[str, Any], execute: bool) -> None:
    repository = recorder.api("repository", f"repos/{REPOSITORY}")
    recorder.check("repository-identity", REPOSITORY, {"full_name": repository.get("full_name")})
    require(repository.get("full_name") == REPOSITORY, "resolved repository mismatch")
    resolved_url = recorder.command(
        "git-remote-url", ["git", "ls-remote", "--get-url", REMOTE_URL]
    ).strip()
    require(resolved_url == REMOTE_URL, "Git URL rewriting would change the fixed remote")
    check_ci(recorder, manifest, "main-ci-preflight")

    branches = manifest["branches"]
    refs = [MAIN_REF, *(branch["ref"] for branch in branches)]
    initial = remote_refs(recorder, refs, "fresh-remote-refs")
    recorder.check("fresh-ref-snapshot", refs, initial)
    require(set(initial) == set(refs), "main or a requested branch is missing")
    for branch in branches:
        require(initial[branch["ref"]] == branch["expected_sha"],
                f"{branch['ref']} has moved from expected_sha")

    with tempfile.TemporaryDirectory(prefix="zil-merged-branch-check-") as directory:
        git = ["git", "--git-dir", directory]
        recorder.command("init-temporary-repository", ["git", "init", "--bare", directory])
        destinations = {ref: f"refs/cleanup/input-{index}" for index, ref in enumerate(refs)}
        recorder.command(
            "fetch-fresh-ancestry",
            [*git, "-c", "gc.auto=0", "fetch", "--no-tags", "--no-write-fetch-head",
             "--filter=blob:none", REMOTE_URL,
             *(f"{ref}:{destinations[ref]}" for ref in refs)],
        )
        for ref in refs:
            actual = recorder.command(
                "fetched-ref-" + ref, [*git, "rev-parse", destinations[ref]]
            ).strip()
            require(actual == initial[ref], f"{ref} moved during fetch")
        accepted = manifest["accepted_head"]
        kind = recorder.command("accepted-object-kind", [*git, "cat-file", "-t", accepted]).strip()
        require(kind == "commit", "accepted_head is not an available commit")
        recorder.command(
            "accepted-head-is-in-current-main",
            [*git, "merge-base", "--is-ancestor", accepted, initial[MAIN_REF]],
        )
        for branch in branches:
            tip = branch["expected_sha"]
            recorder.command(
                "tip-in-current-main-" + branch["ref"],
                [*git, "merge-base", "--is-ancestor", tip, initial[MAIN_REF]],
            )
        push = [
            *git, "push", "--porcelain", "--atomic",
            *(f"--force-with-lease={branch['ref']}:{branch['expected_sha']}"
              for branch in branches),
            REMOTE_URL, *(f":{branch['ref']}" for branch in branches),
        ]
        recorder.value["validated_main"] = initial[MAIN_REF]
        recorder.value["planned_push_argv"] = push
        recorder.check("all-preconditions", manifest, {"validated": True})
        if not execute:
            recorder.value["status"] = "DryRunValidated"
            return

        # Refresh both the completed CI identity and every ref immediately before
        # the one atomic transaction. A changed main requires a new invocation.
        check_ci(recorder, manifest, "main-ci-before-delete")
        final = remote_refs(recorder, refs, "refs-before-delete")
        recorder.check("pre-delete-ref-snapshot", initial, final)
        require(final == initial, "remote refs changed after preflight; refusing deletion")
        recorder.value["mutation_attempted"] = True
        recorder.save()  # A writable receipt is required before mutating anything.
        try:
            recorder.command("atomic-leased-delete", push)
        except Refusal:
            recorder.value["status"] = "DeleteFailedOrUncertain"
            recorder.save()
            # Observe the outcome without retrying the mutation.
            try:
                observed = remote_refs(recorder, refs, "refs-after-failed-delete")
                recorder.check("failed-delete-observation", refs, observed)
            except Exception as observation_error:
                recorder.check("failed-delete-observation-error", refs, str(observation_error))
            raise
        after = remote_refs(recorder, refs, "refs-after-delete")
        recorder.check("post-delete-ref-snapshot", refs, after)
        require(MAIN_REF in after, "main is missing after deletion; record requires investigation")
        require(all(branch["ref"] not in after for branch in branches),
                "a requested ref is present after deletion; do not retry automatically")
        recorder.value["deleted_refs"] = [branch["ref"] for branch in branches]
        recorder.value["status"] = "DeletedAndVerifiedAbsent"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--execute", action="store_true", help="perform the atomic leased deletion")
    args = parser.parse_args()
    if args.receipt.exists() or args.receipt.with_name(args.receipt.name + ".tmp").exists():
        print("Refusing to overwrite an existing receipt or receipt temporary file.", file=sys.stderr)
        return 2
    recorder = Recorder(args.receipt, args.execute, args.manifest)
    code = 1
    try:
        recorder.save()
        manifest, digest = read_manifest(args.manifest)
        recorder.value["manifest_sha256"] = digest
        recorder.check("validated-manifest", str(args.manifest), manifest)
        perform(recorder, manifest, args.execute)
        code = 0
    except Exception as error:
        if recorder.value["status"] not in ("DeleteFailedOrUncertain",):
            recorder.value["status"] = (
                "PostMutationFailure" if recorder.value["mutation_attempted"] else "Refused"
            )
        recorder.value["error"] = f"{type(error).__name__}: {error}"
    finally:
        recorder.value["completed_at"] = utc_now()
        recorder.value["exit_code"] = code
        try:
            recorder.save()
        except OSError as error:
            print(f"Cannot write receipt: {error}; do not retry a mutation blindly.", file=sys.stderr)
            code = 1
    print(json.dumps(
        {"status": recorder.value["status"], "receipt": str(args.receipt),
         "mutation_attempted": recorder.value["mutation_attempted"], "exit_code": code}
    ))
    return code


if __name__ == "__main__":
    raise SystemExit(main())

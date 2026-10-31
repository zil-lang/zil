#!/usr/bin/env python3
"""One reviewed, bounded branch retirement. Dry-run never mutates remote refs."""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

REPO = "zil-lang/zil"
REMOTE = "https://github.com/zil-lang/zil.git"
MAIN = "refs/heads/main"
TASK = "refs/heads/task/construction-status-20261031-cleanup"
WORKFLOW = ".github/workflows/construction-status-20261031-cleanup.yml"
SHA = re.compile(r"[0-9a-f]{40}\Z")
TARGETS = {
    "chore/construction8-resume": "6f7b7679a4fd54df0e0460a7b61c425710ad0953",
    "chore/construction10-cleanup": "acccc78fe1b4719fe7598f8ea5d64812e1e9ff5c",
    "chore/construction10-exact-transport": "d6218fe763f13cac89f52f0c6663344bb916b371",
    "chore/construction10-recovery": "88d3e426af67e48d0fbc041ec406f35b54b17b60",
    "chore/construction11-exact-transport": "a40d12be3767c198d80e8928fac84fe8fb5a5522",
    "integration/construction11": "c0418f532e750e7df2b00d9b7d76da82baa5086c",
    "review/construction11": "61203584c01ede0e3bd25c5d4d694198d0b2aa52",
}
METADATA_FILES = {
    "_zil_goal.md", "data/construction-progress.json", "data/handoff.json",
    ".task/impl/232.md", ".task/impl/233.md", ".task/impl/234.md",
}
TASK_FILES = {WORKFLOW, "ops-audit/cleanup.py", "ops-audit/manifest.json",
              "ops-audit/test_cleanup.py", "ops-audit/SHA256SUMS", "ops-audit/README.md"}


class Refusal(RuntimeError):
    pass


def require(ok, message):
    if not ok:
        raise Refusal(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def now():
    return datetime.now(timezone.utc).isoformat()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()


def nul_paths(raw):
    require(not raw or raw.endswith(b"\0"), "Git path output is not NUL-terminated")
    return [p.decode("utf-8", errors="strict") for p in raw.split(b"\0") if p]


def matches(path, exact, prefixes):
    return path in exact or any(path.startswith(p) for p in prefixes)


def validate_manifest(m):
    require(m.get("schema_version") == 1 and m.get("repository") == REPO, "Wrong manifest identity")
    require(m.get("accepted_head") == "f2572502beb76eb86e9254dea855404e4998a64d", "Unexpected accepted CI head")
    require(m.get("main_ci") == {"run_id": 41198113948, "run_attempt": 1}, "Unexpected accepted CI")
    rows = m.get("branches", [])
    require(len(rows) == len(TARGETS), "Incomplete or extra branch targets")
    actual = {r.get("ref"): r.get("expected_sha") for r in rows}
    require(len(actual) == len(rows) and actual == {"refs/heads/" + k: v for k, v in TARGETS.items()}, "Target refs/tips differ from the reviewed fixed set")
    tags = set()
    for r in rows:
        tag = r.get("archive_tag", "")
        require(tag.startswith("refs/tags/archive/20261031/") and re.fullmatch(r"refs/tags/archive/20261031/[a-z0-9-]+", tag), "Invalid archive tag")
        require(tag not in tags, "Duplicate archive tag")
        tags.add(tag)
        require(r.get("kind") in {"merged", "transport"}, "Unsupported retirement kind")
        require(all(SHA.fullmatch(x) for x in r.get("published_commits", [])), "Invalid published commit")
        if r["kind"] == "transport":
            require(SHA.fullmatch(r.get("base", "")), "Missing exact transport base")
            require(r.get("allowed_files") or r.get("allowed_prefixes"), "Missing transport path boundary")
            require(all(p.endswith("/") for p in r.get("allowed_prefixes", [])), "Prefixes must end at directory boundaries")
    require(m.get("required_main_ancestors") == [TARGETS["review/construction11"]], "The review publication must be required")


class Audit:
    def __init__(self, out, mode, manifest_hash, script_hash):
        self.out = out
        require(not out.exists(), "Refusing to overwrite an earlier receipt directory")
        out.mkdir(parents=True)
        self.value = {"schema_version": 1, "repository": REPO, "started_at": now(),
                      "mode": mode, "status": "Started", "mutation_attempted": False,
                      "manifest_sha256": manifest_hash, "script_sha256": script_hash, "steps": []}
        self.save()

    def save(self):
        p = self.out / "receipt.json"
        temporary = p.with_suffix(".tmp")
        temporary.write_bytes(json_bytes(self.value))
        os.replace(temporary, p)

    def record(self, name, value):
        self.value["steps"].append({"name": name, "at": now(), "result": value})
        self.save()

    def run(self, name, args, *, cwd=None, allowed=(0,), timeout=300):
        index = len(self.value["steps"])
        label = f"{index:03d}-{re.sub('[^a-zA-Z0-9_-]', '-', name)}"
        entry = {"name": name, "argv": args, "cwd": str(cwd) if cwd else None,
                 "started_at": now(), "stdout": label + ".stdout", "stderr": label + ".stderr"}
        self.value["steps"].append(entry)
        self.save()
        environment = dict(os.environ)
        for key in ("GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_NAMESPACE"):
            environment.pop(key, None)
        environment["GIT_TERMINAL_PROMPT"] = "0"
        with (self.out / entry["stdout"]).open("wb") as stdout, (self.out / entry["stderr"]).open("wb") as stderr:
            try:
                p = subprocess.run(args, cwd=cwd, env=environment, stdin=subprocess.DEVNULL,
                                   stdout=stdout, stderr=stderr, timeout=timeout, check=False)
            except (OSError, subprocess.TimeoutExpired) as error:
                entry.update(completed_at=now(), infrastructure_error=str(error))
                self.save()
                raise Refusal(f"{name} failed or timed out; see original logs") from error
        raw = (self.out / entry["stdout"]).read_bytes()
        entry.update(completed_at=now(), exit_code=p.returncode, stdout_sha256=digest(raw),
                     stderr_sha256=digest((self.out / entry["stderr"]).read_bytes()))
        self.save()
        require(p.returncode in allowed, f"{name} exited {p.returncode}; see original logs")
        return raw, p.returncode

    def api(self, name, suffix):
        raw, _ = self.run(name, ["gh", "api", "--hostname", "github.com", "--method", "GET", f"repos/{REPO}/{suffix}"])
        return json.loads(raw)

    def refs(self, name):
        raw, _ = self.run(name, ["git", "ls-remote", "--refs", REMOTE, "refs/heads/*", "refs/tags/*"])
        values = {}
        for line in raw.decode().splitlines():
            parts = line.split()
            require(len(parts) == 2 and SHA.fullmatch(parts[0]), "Malformed remote ref")
            sha, ref = parts
            require(ref not in values and (ref.startswith("refs/heads/") or ref.startswith("refs/tags/")), "Unexpected/duplicate ref")
            values[ref] = sha
        return values


def verify_ci(a, m):
    ci = a.api("accepted-main-ci", f"actions/runs/{m['main_ci']['run_id']}")
    expected = {"id": m["main_ci"]["run_id"], "run_attempt": m["main_ci"]["run_attempt"],
                "status": "completed", "conclusion": "success", "event": "push", "head_branch": "main",
                "head_sha": m["accepted_head"], "path": ".github/workflows/verify-main.yml"}
    require(all(ci.get(k) == v for k, v in expected.items()), "Main CI identity/result changed")
    require(ci.get("repository", {}).get("full_name") == REPO and ci.get("head_repository", {}).get("full_name") == REPO, "CI repository mismatch")
    a.record("verified-main-ci", expected)


def error_counter(report, field):
    require(isinstance(report.get(field), list), f"Missing handoff {field}")
    for row in report[field]:
        require(isinstance(row, dict) and isinstance(row.get("id"), str) and isinstance(row.get("reason"), str), "Malformed handoff issue")
    return Counter((x["id"], x["reason"]) for x in report[field])


def check_records(a, gitdir, m, current, parent):
    checkout = Path(parent) / "records"
    a.run("clone-local-record-check", ["git", "clone", "--shared", "--no-checkout", str(gitdir), str(checkout)])
    reports = {}
    for label, sha in [("baseline", m["accepted_head"]), ("candidate", current)]:
        a.run(label + "-checkout", ["git", "checkout", "-B", "main", sha], cwd=checkout)
        raw, code = a.run(label + "-handoff", ["node", "tools/handoff.mjs", "--check"], cwd=checkout, allowed=(0, 1), timeout=1200)
        report = json.loads(raw)
        require(report.get("main_head") == sha and report.get("current_head") == sha and report.get("authority", {}).get("current_is_main") is True, "Handoff did not inspect the exact main checkout")
        errors = error_counter(report, "errors")
        warnings = error_counter(report, "warnings")
        require((code == 0) == (not errors) and report.get("status") == ("Fail" if errors else "Pass"), "Handoff exit/report mismatch")
        require(not any("infrastructure" in reason.lower() or "timed out" in reason.lower() for _, reason in errors), "Infrastructure failure cannot be grandfathered")
        plan_raw, _ = a.run(label + "-plan-self-test", ["python3", "tools/check-plan.py", "--self-test"], cwd=checkout)
        plan = json.loads(plan_raw)
        require(plan.get("ok") is True and plan.get("tasks") == 14 and len(plan.get("self_tests", [])) == 7 and all(x.get("rejected") is True for x in plan["self_tests"]), "Plan self-test incomplete")
        reports[label] = {"report": report, "errors": errors, "warnings": warnings}
        a.run(label + "-checkout-unchanged", ["git", "diff", "--exit-code", "HEAD"], cwd=checkout)
    base, candidate = reports["baseline"], reports["candidate"]
    def rows(counter):
        return [{"id": k[0], "reason": k[1], "count": n} for k, n in sorted(counter.items())]
    result = {"baseline": m["accepted_head"], "candidate": current,
              "baseline_status": base["report"]["status"], "candidate_status": candidate["report"]["status"],
              "baseline_error_count": sum(base["errors"].values()), "candidate_error_count": sum(candidate["errors"].values()),
              "new_errors": rows(candidate["errors"] - base["errors"]), "resolved_errors": rows(base["errors"] - candidate["errors"]),
              "new_warnings": rows(candidate["warnings"] - base["warnings"]),
              "scope": "Record integrity delta and plan consistency; an inherited Fail remains Fail."}
    cards = {}
    for number in ("232", "233", "234"):
        ids = {number, f".task/impl/{number}.md"}
        found = [s for s in candidate["report"].get("slices", []) if s.get("id") == number]
        cards[number] = {
            "records": len(found),
            "status": found[0].get("status") if len(found) == 1 else None,
            "committed": found[0].get("committed") if len(found) == 1 else None,
            "errors": sum(n for (i, _), n in candidate["errors"].items() if i in ids),
            "warnings": sum(n for (i, _), n in candidate["warnings"].items() if i in ids),
        }
    result["candidate_cards"] = cards
    a.record("record-integrity-delta", result)
    require(not result["new_errors"] and not result["new_warnings"], "New handoff issues; no remote mutation permitted")
    require(all(r["records"] == 1 and r["status"] == "已合入" and r["committed"] is True
                and r["errors"] == 0 and r["warnings"] == 0 for r in cards.values()),
            "232/233/234 must each have one committed merged record and zero handoff issues")
    return result


def validate_snapshot(snapshot, targets):
    require(MAIN in snapshot, "Main is absent")
    for row in targets:
        require(snapshot.get(row["ref"]) == row["expected_sha"], f"Target absent or moved: {row['ref']}")
        existing = snapshot.get(row["archive_tag"])
        require(existing in (None, row["expected_sha"]), f"Archive tag collision: {row['archive_tag']}")


def unrelated_changes(before, after, targets):
    managed = {r[k] for r in targets for k in ("ref", "archive_tag")}
    return [{"ref": ref, "before": before.get(ref), "after": after.get(ref)}
            for ref in sorted(set(before) | set(after)) if ref not in managed and before.get(ref) != after.get(ref)]


def perform(a, m, *, execute, dry_path):
    require(os.environ.get("GITHUB_REPOSITORY") == REPO and os.environ.get("GITHUB_REF") == TASK, "Run only in the reviewed repository task push")
    require(os.environ.get("GITHUB_EVENT_NAME") == "push", "Expected a task push event")
    task_sha = os.environ.get("GITHUB_SHA", "")
    require(SHA.fullmatch(task_sha), "Missing exact workflow SHA")
    raw, _ = a.run("resolved-remote-url", ["git", "ls-remote", "--get-url", REMOTE])
    require(raw.decode().strip() == REMOTE, "Remote URL rewriting is not allowed")
    verify_ci(a, m)
    targets = [*m["branches"], {"ref": TASK, "expected_sha": task_sha, "kind": "task",
               "archive_tag": "refs/tags/archive/20261031/construction-status-cleanup-" + task_sha[:12]}]
    before = a.refs("initial-all-remote-refs")
    validate_snapshot(before, targets)
    current = before[MAIN]
    a.value.update(main=current, targets=targets, initial_refs=before)
    a.save()
    if execute:
        require(dry_path is not None, "Execution requires the prior dry-run receipt")
        dry = json.loads(dry_path.read_bytes())
        require(dry.get("status") == "DryRunValidated" and dry.get("mutation_attempted") is False,
                "Prior dry run was not successful/read-only")
        for k in ("manifest_sha256", "script_sha256", "main", "targets"):
            require(dry.get(k) == a.value[k], f"Dry-run identity changed: {k}")
        require(dry.get("records_checked") is True, "Missing checked handoff delta")
        a.record("bound-dry-run", {"path": str(dry_path), "sha256": digest(dry_path.read_bytes())})
    with tempfile.TemporaryDirectory(prefix="zil-retirement-") as directory:
        repo = Path(directory) / "objects.git"
        a.run("init-isolated-objects", ["git", "init", "--bare", str(repo)])
        g = ["git", "--git-dir", str(repo)]
        refs = [MAIN, *(r["ref"] for r in targets)]
        a.run("fetch-exact-live-refs", [*g, "-c", "gc.auto=0", "fetch", "--no-tags", REMOTE,
                                      *(f"{ref}:refs/audit/{i}" for i, ref in enumerate(refs))], timeout=1200)
        for i, ref in enumerate(refs):
            raw, _ = a.run("fetched-tip-" + str(i), [*g, "rev-parse", f"refs/audit/{i}"])
            require(raw.decode().strip() == before[ref], f"Ref moved during fetch: {ref}")
        for i, sha in enumerate([m["accepted_head"], *m["required_main_ancestors"]]):
            a.run("required-main-ancestor-" + str(i), [*g, "merge-base", "--is-ancestor", sha, current])
        raw, _ = a.run("main-metadata-delta", [*g, "diff", "--no-renames", "--name-only", "-z", m["accepted_head"], current])
        paths = nul_paths(raw)
        require(all(matches(p, METADATA_FILES, ["docs/", "evidence/"]) for p in paths), "Main has untested source/config changes")
        a.record("metadata-only-main-delta", paths)
        for i, row in enumerate(targets):
            tip = row["expected_sha"]
            if row["kind"] == "merged":
                a.run(f"merged-target-{i}", [*g, "merge-base", "--is-ancestor", tip, current])
            else:
                base = row.get("base", current)
                raw, _ = a.run(f"bounded-target-paths-{i}", [*g, "diff", "--no-renames", "--name-only", "-z", base + "..." + tip])
                paths = nul_paths(raw)
                exact = TASK_FILES if row["kind"] == "task" else set(row.get("allowed_files", []))
                prefixes = [] if row["kind"] == "task" else row.get("allowed_prefixes", [])
                require(paths and all(matches(p, exact, prefixes) for p in paths), f"Unexpected target changes: {row['ref']}")
                a.record(f"bounded-target-delta-{i}", paths)
            for j, sha in enumerate(row.get("published_commits", [])):
                a.run(f"published-target-in-main-{i}-{j}", [*g, "merge-base", "--is-ancestor", sha, current])
            if row.get("transport_manifest"):
                raw, _ = a.run(f"transport-manifest-{i}", [*g, "show", f"{tip}:{row['transport_manifest']}"])
                observed = json.loads(raw)
                require([x["sha"] for x in observed["targets"]] == row["published_commits"], "Transport published target changed")
        if not execute:
            a.value["record_gate"] = check_records(a, repo, m, current, directory)
            a.value["records_checked"] = True
            final = a.refs("after-readonly-validation")
            validate_snapshot(final, targets)
            require(final[MAIN] == current, "Main moved while validating records")
            a.value["status"] = "DryRunValidated"
            return
        # Every guard above has passed before the first remote mutation.
        latest = a.refs("before-archive")
        validate_snapshot(latest, targets)
        require(latest[MAIN] == current, "Main moved before archive")
        create = [r for r in targets if r["archive_tag"] not in latest]
        if create:
            command = [*g, "push", "--porcelain", "--atomic",
                       *(f"--force-with-lease={r['archive_tag']}:" for r in create), REMOTE,
                       *(f"{r['expected_sha']}:{r['archive_tag']}" for r in create)]
            a.record("planned-create-only-archives", command)
            a.value["mutation_attempted"] = True
            a.save()
            a.run("create-only-archives", command)
        archived = a.refs("verified-archives-before-delete")
        validate_snapshot(archived, targets)
        require(archived[MAIN] == current, "Main moved after archive; no deletion")
        require(all(archived.get(r["archive_tag"]) == r["expected_sha"] for r in targets), "Archive verification failed")
        verify_ci(a, m)
        immediate = a.refs("immediate-pre-delete")
        validate_snapshot(immediate, targets)
        require(immediate[MAIN] == current and all(immediate.get(r["archive_tag"]) == r["expected_sha"] for r in targets), "Main/archive changed before deletion")
        command = [*g, "push", "--porcelain", "--atomic",
                   *(f"--force-with-lease={r['ref']}:{r['expected_sha']}" for r in targets), REMOTE,
                   *(":" + r["ref"] for r in targets)]
        a.record("planned-atomic-leased-deletion", command)
        a.value["mutation_attempted"] = True
        a.save()
        a.run("atomic-leased-deletion", command)
        after = a.refs("post-delete-all-remote-refs")
        a.value.update(final_refs=after, unrelated_ref_changes=unrelated_changes(immediate, after, targets))
        a.save()
        require(all(r["ref"] not in after and after.get(r["archive_tag"]) == r["expected_sha"] for r in targets), "Deletion/archive postcheck failed; do not retry")
        require(after.get(MAIN) == current, "Main changed during deletion; retain receipt and re-audit")
        a.value["status"] = "ArchivedDeletedAndVerified"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--validated-dry-run", type=Path)
    args = parser.parse_args()
    raw = args.manifest.read_bytes()
    a = Audit(args.out.resolve(), "execute" if args.execute else "dry-run", digest(raw), digest(Path(__file__).read_bytes()))
    code = 1
    try:
        m = json.loads(raw)
        validate_manifest(m)
        (a.out / "manifest.json").write_bytes(raw)
        perform(a, m, execute=args.execute, dry_path=args.validated_dry_run)
        code = 0
    except Exception as error:
        a.value.update(status="MutationFailedOrUncertain" if a.value["mutation_attempted"] else "Refused", error=f"{type(error).__name__}: {error}")
        if a.value["mutation_attempted"]:
            try:
                a.value["failure_observed_refs"] = a.refs("readback-after-failure-no-retry")
            except Exception as read_error:
                a.value["failure_readback_error"] = str(read_error)
    finally:
        a.value.update(completed_at=now(), exit_code=code)
        a.save()
    print(json.dumps({k: a.value[k] for k in ("status", "exit_code", "mutation_attempted")}))
    return code


if __name__ == "__main__":
    raise SystemExit(main())

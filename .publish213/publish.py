#!/usr/bin/env python3
"""One-use, fail-closed ZIL publication. Never reads or prints credentials."""
from __future__ import annotations
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

SHA = re.compile(r"[0-9a-f]{40}\Z")
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
GATE_IDS = ["format", "workspace", "reference", "native_build", "consumer208", "clippy"]

class Refusal(RuntimeError):
    pass

def require(condition, message):
    if not condition:
        raise Refusal(message)

def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()

def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))

def write_json(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)

def safe_output(value):
    return re.sub(r"(https?://)[^/\s@]+@", r"\1[redacted]@", value)

def run(argv, cwd, *, data=None, allowed=(0,), env=None):
    proc = subprocess.run(argv, cwd=cwd, input=data, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, env=env, check=False)
    if proc.returncode not in allowed:
        raise Refusal(f"{argv[0]} failed ({proc.returncode}): " + safe_output(proc.stderr.strip() or proc.stdout.strip()))
    return proc

class Publication:
    def __init__(self, args):
        self.args = args
        self.repo = Path(args.repo).resolve()
        self.manifest_path = Path(args.manifest).resolve()
        self.bundle = Path(args.bundle).resolve()
        self.manifest = read_json(self.manifest_path)
        self.m = self.manifest
        self.manifest_hash = digest(self.manifest_path)
        self.transport = args.transport_sha
        self.validate_manifest()
        self.source = self.m["source"]["sha"]
        self.main = self.m["main"]["ref"]
        self.remote = self.m["remote"]
        self.expected = {self.main: self.m["main"]["expected_sha"],
                         self.m["transport"]["ref"]: self.transport}
        self.expected.update({item["ref"]: item["expected_sha"] for item in self.m["retire"]})
        self.archives = {self.m["main"]["archive_tag"]: self.m["main"]["expected_sha"],
                         self.m["transport"]["archive_tag"]: self.transport}
        self.archives.update({item["archive_tag"]: item["expected_sha"] for item in self.m["retire"]})

    def git(self, *argv, **kwargs):
        return run(["git", *argv], self.repo, **kwargs)

    def validate_manifest(self):
        m = self.m
        require(m.get("schema") == 1, "unsupported manifest schema")
        require(m.get("repository") == self.args.repository, "repository identity differs from manifest")
        require(re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", m["repository"]), "invalid repository identity")
        require(m.get("remote") == "origin", "only the checkout's origin remote is allowed")
        require(SHA.fullmatch(m["source"]["sha"]), "source SHA has not been finalized")
        require(SHA256.fullmatch(m["source"]["bundle_sha256"]), "bundle checksum has not been finalized")
        require(SHA.fullmatch(self.transport or ""), "transport SHA must be the exact triggering commit")
        require(m["main"]["ref"] == "refs/heads/main", "main destination must be refs/heads/main")
        require(m["transport"]["ref"] == "refs/heads/task/213", "transport destination must be refs/heads/task/213")
        require(m["source"]["bundle_ref"].startswith("refs/heads/"), "bundle must advertise a named branch")
        refs = [m["main"]["ref"], m["transport"]["ref"]]
        tags = [m["main"]["archive_tag"], m["transport"]["archive_tag"], m["guard_tag"]]
        require(SHA.fullmatch(m["main"]["expected_sha"]), "invalid original main SHA")
        require(isinstance(m["retire"], list) and m["retire"], "retirement list must be explicit")
        for item in m["retire"]:
            require(item["ref"].startswith("refs/heads/"), "only branch refs can be retired")
            require(SHA.fullmatch(item["expected_sha"]), "retirement SHA must be exact")
            require(item["disposition"] in ("merged", "archive-only"), "every retirement requires an ancestry policy")
            refs.append(item["ref"])
            tags.append(item["archive_tag"])
        require(len(set(refs)) == len(refs), "duplicate or protected branch in retirement list")
        require(len(set(tags)) == len(tags), "archive tag collision")
        for ref in refs + tags + [m["source"]["bundle_ref"]]:
            require(self.git("check-ref-format", ref, allowed=(0, 1)).returncode == 0, "invalid ref name")
        for tag in tags:
            require(tag.startswith("refs/tags/archive/"), "archive tags must use refs/tags/archive/")
        require(m["source"]["sha"] != self.transport, "transport commit must not be the production source")
        require(datetime.datetime.fromisoformat(m["guard_timestamp"]).tzinfo is not None,
                "guard timestamp must include a time zone")

    def local_identity(self):
        require(self.bundle.is_file(), "bundle file is missing")
        require(digest(self.bundle) == self.m["source"]["bundle_sha256"], "bundle SHA256 mismatch")
        advertised = self.git("bundle", "list-heads", str(self.bundle)).stdout.splitlines()
        wanted = f"{self.source} {self.m['source']['bundle_ref']}"
        require(wanted in advertised, "bundle does not advertise the exact source ref/SHA")
        return {"source_sha": self.source, "bundle_sha256": self.m["source"]["bundle_sha256"],
                "manifest_sha256": self.manifest_hash, "transport_sha": self.transport,
                "repository": self.m["repository"]}

    def remote_refs(self):
        # Never query credential configuration or construct authenticated URLs.
        rows = self.git("ls-remote", "--refs", self.remote, "refs/heads/*", "refs/tags/archive/*").stdout.splitlines()
        result = {}
        for row in rows:
            sha, ref = row.split("\t", 1)
            require(SHA.fullmatch(sha), "remote advertised a non-SHA1 object")
            require(ref not in result, "remote advertised a duplicate ref")
            result[ref] = sha
        return result

    def preflight(self, *, guard_must_be_absent=True):
        refs = self.remote_refs()
        actual_heads = {ref: sha for ref, sha in refs.items() if ref.startswith("refs/heads/")}
        if actual_heads != self.expected:
            differences = [ref for ref in sorted(set(actual_heads) | set(self.expected))
                           if actual_heads.get(ref) != self.expected.get(ref)]
            raise Refusal("remote heads drifted: " + ", ".join(differences))
        for tag, sha in self.archives.items():
            require(refs.get(tag) in (None, sha), f"archive tag drifted or is annotated: {tag}")
        if guard_must_be_absent:
            require(self.m["guard_tag"] not in refs, "archive guard already exists; inspect the previous publication")
        return refs

    def ancestor(self, old, new):
        return self.git("merge-base", "--is-ancestor", old, new, allowed=(0, 1)).returncode == 0

    def ancestry(self):
        require(self.ancestor(self.m["main"]["expected_sha"], self.source), "main would not be fast-forward")
        for item in self.m["retire"]:
            contained = self.ancestor(item["expected_sha"], self.source)
            if item["disposition"] == "merged":
                require(contained, f"required merged branch is absent from source: {item['ref']}")
            else:
                require(not contained, f"archive-only research/work was merged into production: {item['ref']}")

    def clean_source(self):
        require(self.git("rev-parse", "HEAD").stdout.strip() == self.source, "checkout is not the exact verified source")
        require(not self.git("status", "--porcelain=v1", "--untracked-files=normal").stdout.strip(),
                "source worktree changed; refusing publication")

    def receipt_identity(self, receipt):
        expected = self.local_identity()
        for key, value in expected.items():
            require(receipt.get(key) == value, f"receipt identity mismatch: {key}")
        require(receipt.get("source_tree") == self.git("rev-parse", self.source + "^{tree}").stdout.strip(),
                "receipt source tree mismatch")

    def prepare(self):
        self.local_identity()
        self.preflight()
        for path in (self.manifest_path, self.bundle, Path(self.args.receipt).resolve()):
            require(not path.is_relative_to(self.repo), "payload and receipts must be outside the source checkout")
        require(not self.git("status", "--porcelain=v1", "--untracked-files=normal").stdout.strip(),
                "checkout contains local changes")
        # Only local private refs are force-refreshed. No remote mutation occurs here.
        refspecs = [f"+{ref}:refs/publish213/observed/{index}" for index, ref in enumerate(sorted(self.expected))]
        self.git("fetch", "--no-tags", self.remote, *refspecs)
        for index, ref in enumerate(sorted(self.expected)):
            actual = self.git("rev-parse", f"refs/publish213/observed/{index}").stdout.strip()
            require(actual == self.expected[ref], f"head changed while fetching: {ref}")
        self.git("bundle", "verify", str(self.bundle))
        self.git("fetch", "--no-tags", str(self.bundle),
                 "+" + self.m["source"]["bundle_ref"] + ":refs/publish213/source")
        require(self.git("rev-parse", "refs/publish213/source").stdout.strip() == self.source, "fetched source SHA mismatch")
        self.ancestry()
        self.preflight()
        self.git("checkout", "--detach", self.source)
        self.clean_source()
        receipt = {"schema": 1, "stage": "prepared", **self.local_identity(),
                   "source_tree": self.git("rev-parse", "HEAD^{tree}").stdout.strip(),
                   "expected_heads": self.expected, "expected_archive_tags": self.archives,
                   "remote_writes": False}
        write_json(self.args.receipt, receipt)
        return receipt

    def check_gates(self):
        receipt_path = Path(self.args.gates).resolve()
        receipt = read_json(receipt_path)
        self.receipt_identity(receipt)
        require(receipt.get("stage") == "verified" and receipt.get("status") == "Pass", "verification did not pass")
        gates = receipt.get("gates", [])
        require([gate.get("id") for gate in gates] == GATE_IDS, "missing or unexpected verification gates")
        for gate in gates:
            require(gate.get("exit_code") == 0, f"verification gate failed: {gate.get('id')}")
            log = receipt_path.parent / gate["log"]
            require(log.resolve().parent == receipt_path.parent, "gate log must be in the receipt directory")
            require(log.is_file() and digest(log) == gate["log_sha256"], "gate log checksum mismatch")

    def archive_guard(self):
        # An archive-only commit, never a parent of production main. Creating its tag
        # in the same transaction as deletions protects every old history even if a
        # concurrent actor changes a canonical no-op tag after our last read.
        tree = self.git("mktree", data="").stdout.strip()
        parents = sorted(set(self.expected.values()) | {self.source})
        argv = ["-c", "user.name=github-actions[bot]", "-c", "user.email=41898282+github-actions[bot]@users.noreply.github.com",
                "commit-tree", tree]
        for sha in parents:
            argv.extend(["-p", sha])
        message = "Archive-only publication guard; never merge into main.\n\n" + json.dumps({
            "manifest_sha256": self.manifest_hash, "source_sha": self.source,
            "original_heads": self.expected}, sort_keys=True, indent=2) + "\n"
        environment = os.environ.copy()
        environment["GIT_AUTHOR_NAME"] = "github-actions[bot]"
        environment["GIT_COMMITTER_NAME"] = "github-actions[bot]"
        environment["GIT_AUTHOR_EMAIL"] = "41898282+github-actions[bot]@users.noreply.github.com"
        environment["GIT_COMMITTER_EMAIL"] = "41898282+github-actions[bot]@users.noreply.github.com"
        environment["GIT_AUTHOR_DATE"] = self.m["guard_timestamp"]
        environment["GIT_COMMITTER_DATE"] = self.m["guard_timestamp"]
        return self.git(*argv, data=message, env=environment).stdout.strip()

    def push(self, updates):
        # Every changed ref has an explicit expected old OID; empty means absent.
        # Atomic support is mandatory. There is deliberately no non-atomic fallback.
        argv = ["push", "--atomic", "--porcelain"]
        for ref, old, new in updates:
            argv.append("--force-with-lease=" + ref + ":" + (old or ""))
        argv.append(self.remote)
        for ref, old, new in updates:
            argv.append((new or "") + ":" + ref)
        result = self.git(*argv)
        return safe_output(result.stdout)

    def publish(self):
        prepared = read_json(self.args.receipt)
        self.receipt_identity(prepared)
        require(prepared.get("stage") == "prepared" and prepared.get("expected_heads") == self.expected,
                "prepared receipt does not match the remote plan")
        require(prepared.get("expected_archive_tags") == self.archives, "prepared archive plan mismatch")
        self.clean_source()
        self.ancestry()
        self.check_gates()
        before = self.preflight()
        guard_sha = self.archive_guard()
        missing = [(tag, None, sha) for tag, sha in sorted(self.archives.items()) if tag not in before]
        tag_push = self.push(missing) if missing else "All canonical archive tags already match.\n"
        archived = self.preflight()
        require(all(archived.get(tag) == sha for tag, sha in self.archives.items()), "canonical archive verification failed")
        # Final transaction: FF main, exact old-head deletions, and a new archive
        # guard. No-op canonical tag refspecs also detect drift at advertisement.
        updates = [(self.main, self.expected[self.main], self.source)]
        updates += [(ref, sha, None) for ref, sha in sorted(self.expected.items()) if ref != self.main]
        updates += [(tag, sha, sha) for tag, sha in sorted(self.archives.items())]
        updates += [(self.m["guard_tag"], None, guard_sha)]
        self.clean_source()
        final_push = self.push(updates)
        after = self.remote_refs()
        actual_heads = {ref: sha for ref, sha in after.items() if ref.startswith("refs/heads/")}
        require(actual_heads == {self.main: self.source}, "post-publication branch verification failed; inspect remote")
        require(all(after.get(tag) == sha for tag, sha in self.archives.items()),
                "post-publication archive drift; guard retains all histories, inspect remote")
        require(after.get(self.m["guard_tag"]) == guard_sha, "archive guard verification failed")
        receipt = {"schema": 1, "stage": "published", **self.local_identity(),
                   "source_tree": prepared["source_tree"], "main_fast_forward": True,
                   "archived": self.archives, "archive_guard": {"ref": self.m["guard_tag"], "sha": guard_sha},
                   "retired": {ref: sha for ref, sha in self.expected.items() if ref != self.main},
                   "remote_heads_after": actual_heads, "tag_push": tag_push, "atomic_publication": final_push}
        write_json(self.args.result, receipt)
        return receipt

    def dry_run(self):
        identity = self.local_identity()
        before = self.preflight()
        available = self.git("cat-file", "-e", self.source + "^{commit}", allowed=(0, 1, 128)).returncode == 0
        if available:
            self.ancestry()
        return {"schema": 1, "stage": "dry-run", **identity, "remote_writes": False,
                "source_ancestry": "Pass" if available else "Deferred until prepare imports the verified bundle",
                "expected_heads": self.expected, "archive_tags": self.archives,
                "new_archive_tags": [tag for tag in self.archives if tag not in before],
                "guard_tag": self.m["guard_tag"], "main_update": "fast-forward only",
                "retire_refs": [ref for ref in self.expected if ref != self.main]}

def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("stage", choices=("dry-run", "prepare", "publish"))
    result.add_argument("--repo", required=True)
    result.add_argument("--manifest", required=True)
    result.add_argument("--bundle", required=True)
    result.add_argument("--repository", required=True)
    result.add_argument("--transport-sha", required=True)
    result.add_argument("--receipt", help="prepare receipt outside the source worktree")
    result.add_argument("--gates", help="verify.py gate receipt")
    result.add_argument("--result", help="final publication receipt")
    return result

def main():
    args = parser().parse_args()
    if args.stage in ("prepare", "publish") and not args.receipt:
        parser().error("--receipt is required")
    if args.stage == "publish" and (not args.gates or not args.result):
        parser().error("publish requires --gates and --result")
    try:
        publisher = Publication(args)
        output = getattr(publisher, args.stage.replace("-", "_"))()
        print(json.dumps(output, ensure_ascii=False, indent=2))
        return 0
    except (Refusal, KeyError, ValueError, OSError) as error:
        print("PUBLICATION REFUSED: " + safe_output(str(error)), file=sys.stderr)
        return 1

if __name__ == "__main__":
    sys.exit(main())

"""Prepare construction 9 archival and compare-and-delete its five task refs.

Default mode is read-only for the remote. --execute is reserved for the root's
approved closing commit after main CI. The transport workflow must replace the
existing importer job, not run beside it.
"""
import argparse
import base64
import hashlib
import json
import pathlib
import re
import subprocess


BASE_HEADS = {
    "task/228": "1082b50ec3221ccf0550255fb1a6675500266055",
    "task/229": "026ea29beb8d83c382ad04bd417606679769afa3",
    "task/228-consumer": "2890f226fdc2328248b341de4cdcaeb49871b0a0",
    "task/228-229": "faf7c8e2c713d81cedaf2b2a74a738045f093cd0",
}
TRANSPORT = "task/228-transport"
ALLOWED = frozenset([*BASE_HEADS, TRANSPORT])
ORIGINAL_TRANSPORT = "cfa425410f864f997cb7cfd3295d07e745344130"
ORIGINAL_RUN = 41154384915
ORIGINAL_RECEIPT_SHA256 = "74eb1f24a2ad4d0b9ffffabe9835d0bf8ed3805b8bb89836ff1e7f1b365809cf"
CI_PATHS = (
    "crates", "Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain",
    "rust-toolchain.toml", ".cargo", "tools", ".github", "policy",
    "stdlib", "std", "spec", "_zil_design.md",
    "examples", "conformance", "validation", "tests", "benches",
    "data/normative-inputs.json",
)


def require(value, message):
    if not value:
        raise ValueError(message)


def hexadecimal(value, size):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{%d}" % size, value)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON key: " + key)
        result[key] = value
    return result


def read_json(raw):
    return json.loads(raw, object_pairs_hook=unique_object)


def git_bytes(root, *args, data=None):
    return subprocess.check_output(["git", *args], cwd=root, input=data)


def git(root, *args, data=None):
    return git_bytes(root, *args, data=data).decode().strip()


def run_git(root, *args):
    subprocess.run(["git", *args], cwd=root, check=True)


def refs(root, names):
    output = git(root, "ls-remote", "--refs", "origin", *names)
    found = {}
    for line in output.splitlines():
        pieces = line.split()
        require(len(pieces) == 2 and hexadecimal(pieces[0], 40), "invalid remote ref advertisement")
        sha, name = pieces
        require(name in names and name not in found, "unexpected or duplicate advertised ref")
        found[name] = sha
    return found


def checked_receipt_file(directory, name):
    require(isinstance(name, str), "receipt path must be a string")
    path = pathlib.PurePosixPath(name)
    require(not path.is_absolute() and ".." not in path.parts and "\\" not in name,
            "receipt must be a child of the snapshot directory")
    current = directory
    for component in path.parts:
        current = current / component
        require(not current.is_symlink(), "symlink receipt path rejected")
    require(current.is_file(), "receipt file missing")
    return current.read_bytes()


def validate(snapshot_path, expected_transport):
    raw_snapshot = snapshot_path.read_bytes()
    snapshot = read_json(raw_snapshot)
    require(isinstance(snapshot, dict) and type(snapshot.get("schema_version")) is int
            and snapshot["schema_version"] == 1, "invalid cleanup snapshot")
    heads = snapshot.get("expected_heads")
    require(isinstance(heads, dict) and set(heads) == set(BASE_HEADS), "snapshot must name exactly the four approved task branches")
    require(all(hexadecimal(sha, 40) for sha in heads.values()), "invalid fixed task head")
    require(hexadecimal(snapshot.get("expected_main"), 40), "final expected main SHA is required")
    transport = snapshot.get("expected_transport")
    if expected_transport:
        require(transport is None or transport == expected_transport, "runtime transport SHA differs from the frozen snapshot")
        transport = expected_transport
    require(hexadecimal(transport, 40), "fixed expected transport SHA is required")
    ci = snapshot.get("main_ci")
    require(isinstance(ci, dict) and type(ci.get("id")) is int and ci["id"] > 0,
            "completed main CI metadata is required")
    require(ci.get("status") == "completed" and ci.get("conclusion") == "success"
            and ci.get("head_branch") == "main" and hexadecimal(ci.get("head_sha"), 40),
            "main CI must have completed successfully for a concrete main commit")
    require(ci.get("path", "").split("@", 1)[0] == ".github/workflows/verify-main.yml",
            "main CI metadata must identify the full main verification workflow")
    entries = snapshot.get("publication_receipts")
    require(isinstance(entries, list) and entries, "publication receipts are required")
    archived, published, run_ids = [], {}, set()
    for index, entry in enumerate(entries):
        require(isinstance(entry, dict) and type(entry.get("run_id")) is int and entry["run_id"] > 0,
                "invalid publication run")
        run_id = entry["run_id"]
        require(run_id not in run_ids and hexadecimal(entry.get("transport_commit"), 40), "duplicate run or invalid transport commit")
        require(hexadecimal(entry.get("sha256"), 64), "receipt SHA-256 is required")
        raw = checked_receipt_file(snapshot_path.parent, entry.get("path"))
        digest = hashlib.sha256(raw).hexdigest()
        require(digest == entry["sha256"], "publication receipt byte digest mismatch")
        receipt = read_json(raw)
        require(receipt.get("status") == "Pass" and receipt.get("mode") == "publication"
                and receipt.get("remote_access") is True, "receipt is not a successful publication")
        require(hexadecimal(receipt.get("bundle_sha256"), 64), "receipt bundle digest is invalid")
        rows = receipt.get("targets")
        require(isinstance(rows, list) and 0 < len(rows) <= len(BASE_HEADS), "invalid publication target list")
        update = {}
        for row in rows:
            branch, sha = row.get("branch"), row.get("sha")
            require(branch in BASE_HEADS and branch not in update and hexadecimal(sha, 40), "publication target is outside the four task refs")
            update[branch] = sha
        if index == 0:
            require(run_id == ORIGINAL_RUN and entry["transport_commit"] == ORIGINAL_TRANSPORT
                    and digest == ORIGINAL_RECEIPT_SHA256 and update == BASE_HEADS,
                    "the original exact publication receipt must be the first archive entry")
        published.update(update)
        archived.append({"run_id": run_id, "transport_commit": entry["transport_commit"],
                         "sha256": digest, "size": len(raw), "encoding": "base64",
                         "bytes": base64.b64encode(raw).decode()})
        run_ids.add(run_id)
    require(published == heads, "publication receipt sequence does not reconstruct the final frozen task heads")
    return snapshot, transport, archived, hashlib.sha256(raw_snapshot).hexdigest()


def check_snapshot(root, expected, archive_ref):
    actual = refs(root, [*expected, archive_ref])
    require({name: actual.get(name) for name in expected} == expected,
            "a frozen remote head moved or is absent; stopping before deletion")
    return actual


def archive_contents(root, transport, archived, heads, tag_name):
    timestamp = git(root, "show", "-s", "--format=%ct", transport)
    require(timestamp.isdigit(), "invalid transport commit time")
    message = {"schema_version": 1, "archived_transport": transport,
               "required_original_transport_ancestor": ORIGINAL_TRANSPORT,
               "final_published_task_heads": heads, "publication_receipts": archived}
    header = ("object " + transport + "\ntype commit\ntag " + tag_name
              + "\ntagger ZIL construction9 archive <construction9-archive@zil.invalid> "
              + timestamp + " +0000\n\n")
    return (header + json.dumps(message, sort_keys=True, indent=2) + "\n").encode()


def verify_remote_archive(root, archive_ref, tag_oid, contents, transport):
    require(refs(root, [archive_ref]) == {archive_ref: tag_oid}, "archive tag is missing or differs")
    local_ref = "refs/construction9-cleanup/archive/" + tag_oid
    run_git(root, "fetch", "--no-tags", "origin", archive_ref + ":" + local_ref)
    require(git(root, "rev-parse", local_ref) == tag_oid and git(root, "cat-file", "-t", tag_oid) == "tag",
            "remote archive is not the expected annotated tag")
    require(git(root, "rev-parse", local_ref + "^{}") == transport, "archive target differs")
    require(git_bytes(root, "cat-file", "tag", tag_oid) == contents, "archive receipt bytes differ")


def cleanup(root, snapshot_path, expected_transport=None, execute=False):
    snapshot, transport, archived, snapshot_digest = validate(snapshot_path, expected_transport)
    heads = dict(snapshot["expected_heads"])
    heads[TRANSPORT] = transport
    require(set(heads) == ALLOWED, "internal cleanup scope mismatch")
    main = snapshot["expected_main"]
    expected = {"refs/heads/main": main, **{"refs/heads/" + name: sha for name, sha in heads.items()}}
    tag_name = "archive/zil-construction9-transport-" + transport
    archive_ref = "refs/tags/" + tag_name
    before = check_snapshot(root, expected, archive_ref)
    namespace = "refs/construction9-cleanup/" + hashlib.sha256(json.dumps(expected, sort_keys=True).encode()).hexdigest()
    refspecs = [ref + ":" + namespace + "/" + ref.removeprefix("refs/heads/") for ref in expected]
    run_git(root, "fetch", "--no-tags", "origin", *refspecs)
    for ref, sha in expected.items():
        local_ref = namespace + "/" + ref.removeprefix("refs/heads/")
        require(git(root, "rev-parse", local_ref) == sha and git(root, "cat-file", "-t", sha) == "commit",
                "head moved during fetch or is not an exact commit")
    for branch, anchor in BASE_HEADS.items():
        run_git(root, "merge-base", "--is-ancestor", anchor, heads[branch])
        run_git(root, "merge-base", "--is-ancestor", heads[branch], main)
    run_git(root, "merge-base", "--is-ancestor", ORIGINAL_TRANSPORT, transport)
    for receipt in archived:
        run_git(root, "merge-base", "--is-ancestor", receipt["transport_commit"], transport)
    ci_head = snapshot["main_ci"]["head_sha"]
    run_git(root, "merge-base", "--is-ancestor", ci_head, main)
    # A final metadata-only D may follow the completed main CI commit.
    run_git(root, "diff", "--quiet", ci_head, main, "--", *CI_PATHS)
    contents = archive_contents(root, transport, archived, snapshot["expected_heads"], tag_name)
    tag_oid = git(root, "hash-object", "-t", "tag", "--stdin", data=contents)
    require(hexadecimal(tag_oid, 40), "invalid proposed tag object ID")
    require(before.get(archive_ref) in (None, tag_oid), "archive tag already exists with different contents")
    delete_refs = ["refs/heads/" + name for name in heads]
    delete_command = ["-c", "push.followTags=false", "-c", "remote.origin.mirror=false", "push", "--atomic",
                      *["--force-with-lease=" + ref + ":" + expected[ref] for ref in delete_refs],
                      "origin", *[":" + ref for ref in delete_refs]]
    report = {"status": "Prepared", "remote_mutations": False, "snapshot_sha256": snapshot_digest,
              "expected_remote_heads": expected, "main_ci": snapshot["main_ci"],
              "ci_equivalence_paths": list(CI_PATHS), "archive_ref": archive_ref,
              "archive_tag_object": tag_oid, "archive_target": transport,
              "original_transport_retained_as_ancestor": ORIGINAL_TRANSPORT,
              "archive_contents_sha256": hashlib.sha256(contents).hexdigest(),
              "publication_receipt_sha256s": [row["sha256"] for row in archived],
              "delete_command": ["git", *delete_command]}
    (root / "construction9-cleanup-plan.json").write_text(json.dumps(report, indent=2) + "\n")
    (root / "construction9-archive-tag.txt").write_bytes(contents)
    if not execute:
        if before.get(archive_ref):
            verify_remote_archive(root, archive_ref, tag_oid, contents, transport)
        return report
    try:
        check_snapshot(root, expected, archive_ref)
        if before.get(archive_ref) is None:
            actual_oid = git(root, "mktag", data=contents)
            require(actual_oid == tag_oid, "written archive tag differs from the reviewed proposal")
            run_git(root, "-c", "push.followTags=false", "-c", "remote.origin.mirror=false",
                    "push", "--atomic", "origin", tag_oid + ":" + archive_ref)
            report["remote_mutations"] = True
        verify_remote_archive(root, archive_ref, tag_oid, contents, transport)
        latest = check_snapshot(root, expected, archive_ref)
        require(latest.get(archive_ref) == tag_oid, "archive changed before deletion")
        report["status"] = "Archived; deletion pending"
        (root / "construction9-cleanup.json").write_text(json.dumps(report, indent=2) + "\n")
        # Every deletion carries its exact expected old SHA; the five updates
        # form one server-side atomic push. No other branch appears in a refspec.
        run_git(root, *delete_command)
        report["remote_mutations"] = True
        after = refs(root, [*expected, archive_ref])
        require(not any(ref in after for ref in delete_refs), "a target branch remains after the atomic deletion")
        require(after.get("refs/heads/main") == main and after.get(archive_ref) == tag_oid,
                "main or archive changed during the final check")
        report["status"] = "Pass"
        report["after"] = after
        return report
    except Exception as error:
        report["status"] = "Stopped"
        report["error"] = str(error)
        raise
    finally:
        (root / "construction9-cleanup.json").write_text(json.dumps(report, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", required=True, type=pathlib.Path)
    parser.add_argument("--expected-transport", help="Exact immutable event SHA, only when the snapshot transport field is null")
    parser.add_argument("--execute", action="store_true", help="Root-approved execution; otherwise prepare and inspect only")
    args = parser.parse_args()
    root = pathlib.Path(git(pathlib.Path.cwd(), "rev-parse", "--show-toplevel")).resolve()
    result = cleanup(root, args.snapshot.resolve(), args.expected_transport, args.execute)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()

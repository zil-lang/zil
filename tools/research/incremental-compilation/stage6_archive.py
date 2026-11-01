#!/usr/bin/env python3
"""Freeze stage6 raw evidence with lossless SHA-addressed text/source blobs.

Original path inventories remain complete. Build products and binary caches are
listed individually with sizes and hashes, rather than copied into the archive.
This tool never runs a compiler or recalculates performance observations.
"""
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import tarfile
import tempfile


THIN_TOOL_COMMIT = "644489605"
BINARY_SUFFIXES = {
    ".o", ".a", ".bc", ".bin", ".rlib", ".rmeta", ".so", ".dylib", ".dll",
    ".exe", ".zobj", ".pyc", ".zsem", ".zsemantic", ".ztyped",
}


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def selected_roots(kind, repo, research):
    stage = research / "stage6"
    if kind == "thin":
        roots = sorted(path for path in stage.iterdir()
                       if path.name.startswith("thin-") or path.name == "target-thin")
        tools = [repo / "tools/research/incremental-compilation/thin_local",
                 repo / "tools/research/incremental-compilation/stage6_bench.py",
                 repo / "changes/research-thin-local-lto.md"]
    elif kind == "merge-screens":
        roots = sorted(path for path in stage.iterdir() if path.name.startswith("merge-screen-"))
        roots += [stage / "merge-screens.log", stage / "run-merge-screens.py"]
        tools = [repo / "tools/research/incremental-compilation/stage6_bench.py"]
    elif kind == "screens":
        names = ["screen-default", "screen-closed", "screen-isolation", "screen-execution.json",
                 "screen-summary.json", "screens.log", "run-screens.py", "o2-author.md",
                 "rust-o2-comparison.md", "rust-cgu-inventory.json", "rust-large8192-workproducts.json",
                 "rust-source"]
        roots = [stage / name for name in names]
        roots += sorted(stage.glob("screen-*.log"))
        tools = [repo / "tools/research/incremental-compilation/stage6_bench.py"]
    else:
        names = ["stage6-review", "stage6-semantic-review", "stage6-semantic",
                 "stage6-session-final-probe-controls", "stage6-session-protocol-smoke",
                 "stage6-session-baseline-smoke", "stage6-baseline-session-probe",
                 "stage6-old-db-ablation", "stage6-analysis", "stage6-algorithms-review"]
        roots = [research / name for name in names]
        roots += sorted(research.glob("stage6-semantic-cache-integration*.log"))
        roots += sorted(research.glob("stage6-session-*.log"))
        roots += sorted(research.glob("reactive-*-tests.log"))
        merge_review = research / "stage6-merge-review"
        roots += sorted(path for path in merge_review.iterdir() if path.is_file())
        roots += sorted(stage.glob("merge-*.log"))
        roots += [stage / name for name in ["merge-author.md", "rust-source", "rust-o2-comparison.md"]]
        roots += [stage / name for name in ["session-preparation.json", "session-preparation-paused.json",
                                           "session-prepared-protocol-controls"]]
        merge_tools = research / "stage6-merge-tools"
        roots += [merge_tools / name for name in ["shape-functional-audit.json", "tool-controls.json",
                  "runtime-failure", "real-failure", "runtime-existing-functional", "real-existing-functional"]]
        roots += [stage / name for name in ["runtime-smoke", "runtime-wired-smoke", "shape-smoke"]]
        roots += [stage / name for name in ["runtime-smoke.log", "runtime-wired-smoke.log", "shape-smoke.log",
                                           "cli-o2.log", "cli-o2-wired.log", "clippy-initial.log", "fmt-initial.log"]]
        tools = [repo / "tools/research/incremental-compilation/stage6_session_probe.rs",
                 repo / "tools/research/incremental-compilation/stage6_session_bench.py",
                 repo / "tools/research/incremental-compilation/stage6_runtime.py",
                 repo / "tools/research/incremental-compilation/stage6_merge_runtime.py",
                 repo / "tools/research/incremental-compilation/stage6_real_pair_bench.py",
                 repo / "tools/research/incremental-compilation/stage6_bench.py"]
    tools.append(Path(__file__).resolve())
    require(all(path.exists() for path in roots + tools),
            "missing selected input: " + ", ".join(str(path) for path in roots + tools if not path.exists()))
    result = []
    for root in roots:
        result.append(("research/" + str(root.relative_to(research)), root))
    for root in tools:
        result.append(("collection-source/" + str(root.relative_to(repo)), root))
    return result


def members(root):
    if root.is_file() or root.is_symlink():
        return [root]
    result = []
    for parent, directories, files in os.walk(root, followlinks=False):
        for directory in list(directories):
            path = Path(parent) / directory
            if path.is_symlink():
                result.append(path)
                directories.remove(directory)
        result.extend(Path(parent) / name for name in files)
    return sorted(result)


def omission(path):
    if path.is_symlink():
        return "symlink-recorded-without-dereference"
    if "__pycache__" in path.parts or path.suffix == ".pyc":
        return "generated-python-bytecode"
    if path.suffix in BINARY_SUFFIXES or ".so." in path.name:
        return "compiled-artifact-or-binary-cache"
    if path.name.endswith(".lock") and "incremental" in path.parts:
        return "ephemeral-rust-incremental-lock"
    if path.name == "llvmcache.timestamp":
        return "ephemeral-thin-cache-timestamp"
    if path.suffix == ".candidate" and path.stat().st_size > 256 * 1024 and "stage6-semantic-review" not in path.parts:
        return "large-functional-semantic-cache-candidate"
    with path.open("rb") as stream:
        magic = stream.read(8)
    if magic.startswith(b"\x7fELF"):
        return "elf-executable-or-object"
    if magic.startswith((b"BC\xc0\xde", b"\xde\xc0\x17\x0b", b"!<arch>\n")):
        return "llvm-bitcode-or-static-archive"
    return None


def inventory(roots):
    rows = []
    seen = set()
    for label, root in roots:
        for path in members(root):
            name = label if root.is_file() or root.is_symlink() else label + "/" + str(path.relative_to(root))
            require(name not in seen, "duplicate selected path: " + name)
            seen.add(name)
            reason = omission(path)
            row = {"path": name, "source_path": str(path), "bytes": path.lstat().st_size,
                   "mode": stat.S_IMODE(path.lstat().st_mode),
                   "disposition": "omit" if reason else "retain"}
            if reason:
                row["reason"] = reason
            if path.is_symlink():
                row["link_target"] = os.readlink(path)
            rows.append(row)
    return rows


def summarize(rows):
    counts, sizes = Counter(), Counter()
    for row in rows:
        key = row.get("reason", "retained-raw-evidence")
        counts[key] += 1
        sizes[key] += row["bytes"]
    return [{"class": key, "files": counts[key], "bytes": sizes[key]} for key in sorted(counts)]


def git_sources(repo, kind):
    if kind != "thin":
        return []
    paths = ["tools/research/incremental-compilation/thin_local",
             "tools/research/incremental-compilation/stage6_bench.py"]
    commit = subprocess.check_output(["git", "rev-parse", THIN_TOOL_COMMIT], cwd=repo, text=True).strip()
    names = subprocess.check_output(["git", "ls-tree", "-r", "--name-only", commit, "--", *paths],
                                    cwd=repo, text=True).splitlines()
    result = []
    for name in names:
        data = subprocess.check_output(["git", "show", f"{commit}:{name}"], cwd=repo)
        result.append(({"path": f"frozen-tools/{commit}/{name}", "bytes": len(data),
                        "source_git_commit": commit, "source_git_path": name,
                        "disposition": "retain", "sha256": digest(data)}, data))
    return result


def add_bytes(bundle, name, data):
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mtime = 0
    info.mode = 0o644
    bundle.addfile(info, io.BytesIO(data))


def collect(args, roots, rows):
    require(args.out is not None and not args.out.exists(), "new --out directory required")
    args.out.mkdir(parents=True)
    root_receipt = [{"archive_prefix": label, "source_path": str(path)} for label, path in roots]
    write_json(args.out / "ROOTS.json", root_receipt)
    write_json(args.out / "PLAN.json", {"kind": args.kind, "roots": root_receipt, "summary": summarize(rows)})
    retained, omitted, blobs = [], [], {}
    print(json.dumps({"phase": "hashing", "kind": args.kind, "files": len(rows)}), flush=True)
    for row in rows:
        path = Path(row["source_path"])
        before = path.lstat()
        require(before.st_size == row["bytes"] and stat.S_IMODE(before.st_mode) == row["mode"],
                "input changed after inventory: " + str(path))
        row["sha256"] = digest(os.readlink(path).encode()) if path.is_symlink() else sha(path)
        row["sha256_scope"] = "symlink-target-text" if path.is_symlink() else "file-bytes"
        after = path.lstat()
        require((before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns),
                "input changed while hashing: " + str(path))
        if row["disposition"] == "omit":
            omitted.append(row)
        else:
            retained.append(row)
            blobs.setdefault(row["sha256"], (path, None, row["bytes"]))
    for row, data in git_sources(args.repo, args.kind):
        retained.append(row)
        blobs.setdefault(row["sha256"], (None, data, row["bytes"]))
    for row in retained:
        row["archive_member"] = "blobs/sha256/" + row["sha256"]
    write_json(args.out / "FILES.json", retained)
    write_json(args.out / "OMITTED.json", omitted)
    print(json.dumps({"phase": "compressing", "kind": args.kind, "retained_paths": len(retained),
                      "unique_blobs": len(blobs), "unique_bytes": sum(item[2] for item in blobs.values())}), flush=True)
    archive_path = args.out / "raw.tar.gz"
    with tarfile.open(archive_path, "w:gz", compresslevel=1) as bundle:
        for key, (path, data, size) in sorted(blobs.items()):
            payload = path.read_bytes() if path is not None else data
            require(len(payload) == size and digest(payload) == key,
                    "retained file changed before archive: " + str(path))
            add_bytes(bundle, "blobs/sha256/" + key, payload)
        for name in ["FILES.json", "OMITTED.json", "ROOTS.json", "PLAN.json"]:
            add_bytes(bundle, name, (args.out / name).read_bytes())
    identity = {
        "schema": 1, "kind": args.kind, "collected_utc": datetime.now(timezone.utc).isoformat(),
        "head_at_collection": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.repo, text=True).strip(),
        "collector_sha256": sha(Path(__file__)),
        "boundary": "Raw archived observations, not rerun measurements or final performance acceptance. Each original path maps to exact bytes in a SHA-addressed blob. Duplicate raw bytes are stored once without rewriting text, JSON, LLVM IR, source or receipts. Current collection-source files are distinct from measured request-local or frozen-tool copies. Omitted build products/cache binaries are individually inventoried with their observed byte SHA; original tool/source/request identities remain authoritative.",
        "archive": {"path": archive_path.name, "bytes": archive_path.stat().st_size, "sha256": sha(archive_path)},
        "sidecars": {name: sha(args.out / name) for name in ["FILES.json", "OMITTED.json", "ROOTS.json", "PLAN.json"]},
        "retained_paths": len(retained), "unique_blobs": len(blobs),
        "unique_retained_bytes": sum(item[2] for item in blobs.values()),
        "omitted_paths": len(omitted), "summary": summarize(retained + omitted),
        "outside_selected_scope": ([{
            "path": str(args.research / "stage6-merge-review/exact"),
            "reason": "Reviewer source worktree excluded by integration instruction; retained review/commit receipts bind the reviewed commit and source hashes.",
        }, {
            "path": str(args.research / "stage6-merge-review/target"),
            "reason": "Reviewer build target excluded by integration instruction; test commands/logs and source identity are retained.",
        }, {
            "path": str(args.research / "stage6/session-db-isolation"),
            "reason": "Separate formal performance window; outside this frozen correctness collection.",
        }, {
            "path": str(args.research / "stage6/session-final-vs-3d6"),
            "reason": "Separate formal performance window; outside this frozen correctness collection.",
        }] if args.kind == "correctness" else []),
        "reconstruction": "Use stage6_archive.py --extract <archive-directory> --dest <new-directory>. Only retained files are reconstructed. Omitted binary cache/tool/executable bytes cannot be recovered from this archive; rebuild using the preserved source and original receipts if needed.",
    }
    write_json(args.out / "IDENTITY.json", identity)
    print(json.dumps(identity, indent=2))


def split_archive(root):
    identity = json.loads((root / "IDENTITY.json").read_text())
    original = root / identity["archive"]["path"]
    require("parts" not in identity["archive"], "archive is already split")
    require(sha(original) == identity["archive"]["sha256"], "original archive SHA differs")
    parts = []
    with original.open("rb") as stream:
        while data := stream.read(40 * 1024 * 1024):
            name = f"{original.name}.part-{len(parts) + 1:03d}"
            path = root / name
            require(not path.exists(), "part already exists")
            path.write_bytes(data)
            require(sha(path) == digest(data), "written part SHA differs")
            parts.append({"path": name, "bytes": len(data), "sha256": digest(data)})
    combined = hashlib.sha256()
    for part in parts:
        combined.update((root / part["path"]).read_bytes())
    require(combined.hexdigest() == identity["archive"]["sha256"], "split stream differs")
    identity["archive"]["parts"] = parts
    identity["archive"]["storage"] = "Ordered byte segments of the original gzip stream; concatenate in listed order before ordinary tar extraction, or use this collector's --extract."
    identity["packaging_tool_sha256"] = sha(Path(__file__))
    write_json(root / "IDENTITY.json", identity)
    original.unlink()
    print(json.dumps({"split": str(root), "parts": parts}, indent=2))


def verify(root, destination=None):
    identity = json.loads((root / "IDENTITY.json").read_text())
    archive_stream = tempfile.TemporaryFile()
    parts = identity["archive"].get("parts", [identity["archive"]])
    combined = hashlib.sha256()
    for part in parts:
        path = root / part["path"]
        require(path.stat().st_size == part["bytes"] and sha(path) == part["sha256"],
                "archive part SHA mismatch")
        with path.open("rb") as stream:
            while data := stream.read(1024 * 1024):
                combined.update(data)
                archive_stream.write(data)
    require(combined.hexdigest() == identity["archive"]["sha256"], "archive SHA mismatch")
    archive_stream.seek(0)
    for name, expected in identity["sidecars"].items():
        require(sha(root / name) == expected, "sidecar SHA mismatch: " + name)
    files = json.loads((root / "FILES.json").read_text())
    by_blob = {}
    for row in files:
        by_blob.setdefault(row["archive_member"], []).append(row)
    if destination:
        require(not destination.exists(), "extract destination already exists")
        destination.mkdir(parents=True)
    with tarfile.open(fileobj=archive_stream, mode="r:gz") as bundle:
        require(set(bundle.getnames()) == set(by_blob) | set(identity["sidecars"]), "archive members differ")
        for member in bundle:
            data = bundle.extractfile(member).read()
            if member.name in identity["sidecars"]:
                require(data == (root / member.name).read_bytes(), "internal sidecar differs")
                continue
            for row in by_blob[member.name]:
                require(len(data) == row["bytes"] and digest(data) == row["sha256"], "raw blob differs")
                if destination:
                    relative = Path(row["path"])
                    require(not relative.is_absolute() and ".." not in relative.parts, "unsafe archive path")
                    target = destination / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(data)
                    if "mode" in row:
                        target.chmod(row["mode"] & 0o777)
    result = {"kind": identity["kind"], "status": "AllBlobsAndSidecarsMatch", "paths": len(files),
              "unique_blobs": len(by_blob), "omitted_paths": identity["omitted_paths"],
              "identity_sha256": sha(root / "IDENTITY.json"),
              "verified_utc": datetime.now(timezone.utc).isoformat()}
    if not destination:
        write_json(root / "VERIFY.json", result)
    archive_stream.close()
    print(json.dumps(result, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kind", choices=["thin", "screens", "merge-screens", "correctness"])
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--research", type=Path, default=Path("/workspace/.zil-env/research"))
    parser.add_argument("--out", type=Path)
    parser.add_argument("--plan", type=Path)
    parser.add_argument("--verify", type=Path)
    parser.add_argument("--extract", type=Path)
    parser.add_argument("--dest", type=Path)
    parser.add_argument("--split-existing", type=Path)
    args = parser.parse_args()
    if args.split_existing:
        split_archive(args.split_existing)
        return
    if args.verify or args.extract:
        require(not args.extract or args.dest, "--extract needs --dest")
        verify(args.verify or args.extract, args.dest if args.extract else None)
        return
    require(args.kind is not None, "--kind is required")
    roots = selected_roots(args.kind, args.repo.resolve(), args.research.resolve())
    rows = inventory(roots)
    if args.plan:
        write_json(args.plan, {"kind": args.kind,
                              "roots": [{"prefix": label, "source_path": str(path)} for label, path in roots],
                              "summary": summarize(rows), "files": rows})
        print(json.dumps({"kind": args.kind, "summary": summarize(rows), "plan": str(args.plan)}, indent=2))
    else:
        collect(args, roots, rows)


if __name__ == "__main__":
    main()

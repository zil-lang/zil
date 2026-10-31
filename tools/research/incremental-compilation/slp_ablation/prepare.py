#!/usr/bin/env python3
"""Reproduce the ineffective SLP setter experiment on the frozen Merge candidate.

LLVM 23.1.4's default<O2> alias resets this requested setting to true. This is
retained as a failed-control reproduction, not an effective SLP-off pipeline.
"""
import argparse
import difflib
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile

REVISION = "43037eb7b108ba864144979204b5c6d18636dac3"
TREE = "55968984013f78fcc3d1af8f79f6741c44af8e17"
REFERENCE_BINARY_SHA256 = "57451389969cb24ef755b1da81aa0ba879c9bbd11bba2a012f1c6f3f269f8787"
NATIVE = "crates/zil-llvm-api/src/native.rs"
LIB = "crates/zil-llvm-api/src/lib.rs"


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--materialize", action="store_true",
                        help="Also unpack build sources; still never compile.")
    args = parser.parse_args()

    def git(*parts):
        return subprocess.check_output(["git", *parts], cwd=args.repo)

    assert git("rev-parse", REVISION).decode().strip() == REVISION
    assert git("rev-parse", REVISION + "^{tree}").decode().strip() == TREE
    args.out.mkdir(parents=True, exist_ok=False)
    roots = ["Cargo.toml", "Cargo.lock", "crates", "tools/llvm/version.txt"]
    root_names = git("ls-tree", "--name-only", REVISION).decode().splitlines()
    if "rust-toolchain.toml" in root_names:
        roots.append("rust-toolchain.toml")
    tree_entries = []
    for record in git("ls-tree", "-rz", REVISION, "--", *roots).split(b"\0"):
        if not record:
            continue
        metadata, path = record.split(b"\t", 1)
        mode, kind, blob = metadata.decode().split()
        assert kind == "blob"
        tree_entries.append({"path": path.decode(), "mode": mode, "blob": blob})

    edits = []
    patch = []
    replacements = {
        NATIVE: [("LLVMPassBuilderOptionsSetSLPVectorization(opts.0, i32::from(optimized));",
                  "LLVMPassBuilderOptionsSetSLPVectorization(opts.0, 0);")],
        LIB: [(f'"{identity}"', f'"{identity}|slp-vectorization=false"') for identity in [
            "default<O2>",
            "default<O2>|merge-functions=after|aliases=false",
            "mergefunc,default<O2>|merge-functions=after|aliases=false",
        ]],
    }
    for path, pairs in replacements.items():
        before = git("show", REVISION + ":" + path).decode()
        after = before
        for old, new in pairs:
            assert after.count(old) == 1, (path, old)
            after = after.replace(old, new)
        for directory, text in [("original", before), ("patched", after)]:
            dest = args.out / directory / path
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(text)
        patch.extend(difflib.unified_diff(before.splitlines(True), after.splitlines(True),
                                         fromfile="a/" + path, tofile="b/" + path))
        edits.append({"path": path, "original_sha256": sha(before.encode()),
                      "patched_sha256": sha(after.encode()),
                      "scope": "sole codegen behavior delta" if path == NATIVE
                      else "receipt/cache pipeline identity only"})
    diff = "".join(patch)
    (args.out / "RESEARCH.patch").write_text(diff)

    source_manifest = None
    if args.materialize:
        source = args.out / "source"
        source.mkdir()
        archive = git("archive", "--format=tar", REVISION, "--", *roots)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(source, filter="data")
        source_manifest = []
        for entry in tree_entries:
            path = source / entry["path"]
            assert path.is_file() and not path.is_symlink()
            source_manifest.append({**entry, "sha256": sha(path.read_bytes()),
                                    "bytes": path.stat().st_size})
        for edit in edits:
            path = source / edit["path"]
            assert sha(path.read_bytes()) == edit["original_sha256"]
            path.write_bytes((args.out / "patched" / edit["path"]).read_bytes())
        (args.out / "SOURCE-ORIGINALS.json").write_text(
            json.dumps(source_manifest, indent=2) + "\n")
    receipt = {
        "revision": REVISION, "tree": TREE,
        "reference_binary_sha256": REFERENCE_BINARY_SHA256,
        "source_materialized": args.materialize, "compilation_performed": False,
        "production_mutated": False, "new_cli_options": False,
        "patch_sha256": sha(diff.encode()), "prepare_sha256": sha(Path(__file__).read_bytes()),
        "modified_files": edits, "original_git_entries": tree_entries,
        "source_manifest_sha256": sha((args.out / "SOURCE-ORIGINALS.json").read_bytes())
            if source_manifest is not None else None,
        "behavior_delta": "Requests SLP=false through the C API; LLVM 23.1.4 default<O2> subsequently resets it to true. This is NOT an effective SLP-off ablation.",
        "cache_boundary": "Distinct compiler executable identity plus explicit SLP-off pipeline_identity; use separate private caches per binary",
        "build_command": "source /workspace/.zil-env/env.sh; CARGO_TARGET_DIR=<independent-target> cargo build --offline --locked --release -p zil-cli --manifest-path <out>/source/Cargo.toml",
        "boundary": "Historical failed-control reproduction. Original frozen receipts retain their initial wording; slp-ablation-outcome.json records the actual alias override. Effective SLP controls use the separately recorded execution-level vectorize-slp option.",
    }
    (args.out / "RESEARCH.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({k: v for k, v in receipt.items() if k != "original_git_entries"}, indent=2))


if __name__ == "__main__":
    main()

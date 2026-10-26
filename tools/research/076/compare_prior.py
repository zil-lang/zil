#!/usr/bin/env python3
"""Verify immutable071 raw members and compare all surviving control canonical files."""
import argparse
import hashlib
import json
from pathlib import Path
import tarfile


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    archive, root, receipt = Path(args.archive), Path(args.out), Path(args.receipt)
    expected_sha = "1788c5d1ac914e2b3b893b8d40e1e1d9562ed4946a4269c4db6e124b26097e44"
    if sha(archive.read_bytes()) != expected_sha or receipt.exists():
        raise RuntimeError("require exact immutable071 archive and a new receipt")
    same = []
    controls = set(json.loads((root / "source/tools/research/071/cases.json").read_text())["methods"])
    with tarfile.open(archive, "r:gz") as tar:
        manifest = json.load(tar.extractfile("research/071/portable-manifest.json"))
        expected = {"research/071/" + name for name in manifest} | {"research/071/portable-manifest.json"}
        actual = [member.name for member in tar.getmembers()]
        if len(actual) != len(expected) or set(actual) != expected:
            raise RuntimeError("071 archive member census mismatch")
        for name, record in manifest.items():
            data = tar.extractfile("research/071/" + name).read()
            if len(data) != record["bytes"] or sha(data) != record["sha256"]:
                raise RuntimeError("071 archive content mismatch: " + name)
            if name.startswith("frozen-001/freeze/graph/") and name.endswith(".json") and "/relations/" in name:
                relative = Path(name).relative_to("frozen-001/freeze/graph")
                if relative.parts[1] not in controls:
                    continue
                current = root / "freeze/graph" / relative
                if not current.exists() or current.read_bytes() != data:
                    raise RuntimeError("frozen071 control canonical changed: " + str(relative))
                same.append(str(relative))
        old_identity = json.load(tar.extractfile("research/071/frozen-001/identity.json"))
        old_sources = json.load(tar.extractfile("research/071/frozen-001/source-manifest.json"))
        controls_source = {name: digest for name, digest in old_sources.items() if name.startswith("tools/research/071/")}
        for name, digest in controls_source.items():
            if sha((root / "source" / name).read_bytes()) != digest:
                raise RuntimeError("immutable071 source changed: " + name)
    current_identity = json.loads((root / "identity.json").read_text())
    result = dict(archive_sha256=expected_sha, verified_raw_members=len(manifest), same_canonical_files=len(same), canonical_files=same, unchanged071_sources=controls_source, old_research_commit=old_identity["research_commit"], current_research_commit=current_identity["research_commit"], old_timing_identity_preserved=True, timing_comparison="none; only exact source and canonical bytes are compared across071 and076")
    receipt.parent.mkdir(parents=True, exist_ok=True)
    receipt.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({key:result[key] for key in ("verified_raw_members", "same_canonical_files", "old_research_commit", "current_research_commit")}))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Package raw067 receipts without reproducible source/target trees; verify every member."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    root = Path(args.root).resolve()
    archive = root / "portable-raw-067.tar.gz"
    manifest = root / "portable-manifest.json"
    if archive.exists() or manifest.exists():
        raise RuntimeError("new archive required; never overwrite raw evidence")
    members = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if path.is_file() and not any(part in ("source", "target") for part in relative.parts):
            members[str(relative)] = {"sha256": sha(path.read_bytes()), "bytes": path.stat().st_size}
    payload = (json.dumps(members, sort_keys=True, indent=2) + "\n").encode()
    manifest.write_bytes(payload)
    with tarfile.open(archive, "w:gz") as tar:
        for name in [*members, "portable-manifest.json"]:
            data = (root / name).read_bytes()
            item = tarfile.TarInfo("research/067/" + name)
            item.size = len(data)
            item.mtime = 0
            item.mode = 0o644
            tar.addfile(item, io.BytesIO(data))
    verified = 0
    with tarfile.open(archive, "r:gz") as tar:
        actual = {member.name for member in tar.getmembers()}
        expected = {"research/067/" + name for name in [*members, "portable-manifest.json"]}
        if actual != expected:
            raise RuntimeError("archive member census mismatch")
        for name, record in members.items():
            stream = tar.extractfile("research/067/" + name)
            if stream is None:
                raise RuntimeError("archive file missing")
            data = stream.read()
            if len(data) != record["bytes"] or sha(data) != record["sha256"]:
                raise RuntimeError("archive content mismatch: " + name)
            verified += 1
        if tar.extractfile("research/067/portable-manifest.json").read() != payload:
            raise RuntimeError("archive manifest mismatch")
    result = dict(archive="out/runs/research/067/portable-raw-067.tar.gz", sha256=sha(archive.read_bytes()), archive_bytes=archive.stat().st_size, manifest_sha256=sha(payload), raw_files=len(members), raw_bytes=sum(record["bytes"] for record in members.values()), verified_members=verified, member_prefix="research/067", formal_data="frozen-001", frozen_source=json.loads((root / "frozen-001/identity.json").read_text())["research_commit"], earlier_attempts_preserved=["dev-001", "dev-002", "dev-003", "dev-004"], source_and_target_excluded=True)
    receipt = Path(args.receipt)
    receipt.parent.mkdir(parents=True, exist_ok=True)
    receipt.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()

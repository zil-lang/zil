#!/usr/bin/env python3
"""Lossless named-root evidence archive; verify every member, keep originals."""
import argparse
import gzip
import hashlib
import io
import json
import re
import stat
import subprocess
import tarfile
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--root", action="append", required=True)
    p.add_argument("--controller", required=True)
    args = p.parse_args()
    repo = Path(__file__).resolve().parents[3]
    assert subprocess.check_output(["git", "show", args.controller + ":tools/research/123/archive.py"], cwd=repo) == Path(__file__).read_bytes()
    roots = {}
    for item in args.root:
        name, directory = item.split("=", 1)
        assert re.fullmatch(r"[a-zA-Z0-9_-]+", name) and name not in roots
        roots[name] = Path(directory).resolve()
    dest = args.out.resolve()
    dest.parent.mkdir(parents=True, exist_ok=True)
    assert not dest.exists()
    members, excluded = {}, []
    with dest.open("xb") as output, gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=0, compresslevel=6) as compressed:
        with tarfile.open(fileobj=compressed, mode="w") as tar:
            for name, root in roots.items():
                for path in sorted(root.rglob("*")):
                    relative = path.relative_to(root)
                    if relative.parts[0].startswith("target-"):
                        if len(relative.parts) == 1:
                            excluded.append(dict(path=str(path), reason="reproducible Cargo target cache; build command/source/error originals retained"))
                        continue
                    if not path.is_file():
                        continue
                    resolved = path.resolve()
                    assert any(resolved.is_relative_to(r) for r in roots.values()), (path, "unexpected external symlink")
                    key = name + "/" + relative.as_posix()
                    assert key not in members
                    before = path.stat()
                    data = path.read_bytes()
                    member = tarfile.TarInfo(key)
                    member.size = len(data)
                    member.mode = stat.S_IMODE(before.st_mode)
                    member.mtime = before.st_mtime
                    tar.addfile(member, io.BytesIO(data))
                    members[key] = dict(bytes=len(data), sha256=sha(data), mode=member.mode,
                                        mtime_ns=before.st_mtime_ns, ctime_ns=before.st_ctime_ns)
                print("archived root", name, "members", len(members), flush=True)
    seen = set()
    with tarfile.open(dest, "r:gz") as tar:
        for member in tar:
            assert member.isfile() and member.name in members and member.name not in seen
            assert not Path(member.name).is_absolute() and ".." not in Path(member.name).parts
            data = tar.extractfile(member).read()
            expected = members[member.name]
            assert len(data) == expected["bytes"] and sha(data) == expected["sha256"] and member.mode == expected["mode"]
            seen.add(member.name)
    assert seen == set(members)
    manifest = dest.with_name(dest.name + ".members.json.gz")
    raw_manifest = json.dumps(members, sort_keys=True, separators=(",", ":")).encode()
    manifest.write_bytes(gzip.compress(raw_manifest, compresslevel=6, mtime=0))
    assert gzip.decompress(manifest.read_bytes()) == raw_manifest
    descriptor = dict(archive=dest.name, sha256=sha(dest.read_bytes()), compressed_bytes=dest.stat().st_size,
        members=manifest.name, members_sha256=sha(manifest.read_bytes()), members_uncompressed_sha256=sha(raw_manifest),
        member_count=len(members), original_bytes=sum(v["bytes"] for v in members.values()), roots={k:str(v) for k,v in roots.items()},
        controller=args.controller, controller_sha256=sha(Path(__file__).read_bytes()), excluded=excluded,
        verified_every_member=True, originals_kept=True, symlinks="materialized only when target lies inside named evidence roots")
    dest.with_name(dest.name + ".json").write_text(json.dumps(descriptor, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k:descriptor[k] for k in ["archive", "sha256", "compressed_bytes", "member_count", "original_bytes"]}), flush=True)


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("assertions require ordinary Python")
    main()

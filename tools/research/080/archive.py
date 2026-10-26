#!/usr/bin/env python3
"""Preserve actual runs; omit only rebuildable frozen source/build directories."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    p=argparse.ArgumentParser()
    p.add_argument("--root",type=Path,required=True)
    p.add_argument("--receipt",type=Path,required=True)
    a=p.parse_args()
    root=a.root.resolve()
    archive=root/"portable-raw-080.tar.gz"
    if archive.exists():raise RuntimeError("refuse to replace an earlier archive")
    omitted=[];members=[]
    for path in sorted(root.rglob("*")):
        if not path.is_file():continue
        relative=path.relative_to(root)
        skip=any(part in {"source-baseline","source-candidate","target-baseline","target-candidate","runner-target","runner-clippy-target"} for part in relative.parts)
        row=dict(path=str(relative),bytes=path.stat().st_size,sha256=sha(path))
        (omitted if skip else members).append(row)
    manifest=dict(schema=1,scope="080 actual development failures, complete controls and original performance samples",
                  members=members,omitted_rebuildable=omitted,
                  omitted_reason="source is reconstructible from frozen exact Git identities; target files from logged toolchain commands and inventories")
    raw=(json.dumps(manifest,sort_keys=True,indent=2)+"\n").encode()
    with tarfile.open(archive,"w:gz",compresslevel=6) as tar:
        info=tarfile.TarInfo("MANIFEST.json");info.size=len(raw);info.mtime=0
        tar.addfile(info,io.BytesIO(raw))
        for row in members:tar.add(root/row["path"],arcname=row["path"],recursive=False)
    with tarfile.open(archive) as tar:
        for row in members:
            content=tar.extractfile(row["path"]).read()
            assert len(content)==row["bytes"] and hashlib.sha256(content).hexdigest()==row["sha256"]
    receipt=dict(status="Pass",archive=str(archive),bytes=archive.stat().st_size,sha256=sha(archive),
                 member_count=len(members),omitted_rebuildable_count=len(omitted),manifest_sha256=hashlib.sha256(raw).hexdigest())
    a.receipt.parent.mkdir(parents=True,exist_ok=True)
    a.receipt.write_text(json.dumps(receipt,indent=2)+"\n")
    print(json.dumps(receipt,indent=2))

if __name__=="__main__":main()

#!/usr/bin/env python3
"""Stream every archived byte against the included manifest, without extraction."""
import argparse
import hashlib
import json
from pathlib import Path
import tarfile

parser=argparse.ArgumentParser();parser.add_argument("--archive",type=Path,required=True)
parser.add_argument("--receipt",type=Path,required=True);args=parser.parse_args()
observed={};manifest=None;manifest_sha=None
with tarfile.open(args.archive,"r|gz") as archive:
    for member in archive:
        name=Path(member.name)
        assert member.isfile() and not name.is_absolute() and ".." not in name.parts and name.parts[0]=="raw"
        relative=str(Path(*name.parts[1:]));stream=archive.extractfile(member)
        if relative=="archive-manifest.json":
            assert manifest is None
            data=stream.read();manifest=json.loads(data);manifest_sha=hashlib.sha256(data).hexdigest()
        else:
            assert relative not in observed
            observed[relative]=dict(bytes=member.size,sha256=hashlib.file_digest(stream,"sha256").hexdigest())
assert manifest is not None and observed==manifest["files"]
with args.archive.open("rb") as stream: digest=hashlib.file_digest(stream,"sha256").hexdigest()
result=dict(status="Pass",archive=str(args.archive.resolve()),archive_sha256=digest,manifest_sha256=manifest_sha,
            verified_files=len(observed),verified_bytes=sum(v["bytes"] for v in observed.values()),
            argv=["python3","tools/research/084/verify_archive.py","--archive",str(args.archive),"--receipt",str(args.receipt)])
args.receipt.parent.mkdir(parents=True,exist_ok=True);args.receipt.write_text(json.dumps(result,indent=2)+"\n")
print(json.dumps(result))

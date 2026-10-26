#!/usr/bin/env python3
"""Archive exact frozen sources, inputs, binaries, full outputs and every sample."""
import argparse
import hashlib
import json
from pathlib import Path
import tarfile

def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream,"sha256").hexdigest()

parser=argparse.ArgumentParser()
parser.add_argument("--out",type=Path,required=True)
parser.add_argument("--archive",type=Path,required=True)
parser.add_argument("--receipt",type=Path,required=True)
args=parser.parse_args()
root=args.out.resolve();archive=args.archive.resolve();receipt=args.receipt.resolve()
assert not archive.exists(),"never replace an archive"
identity=json.loads((root/"identity.json").read_text())
finish=json.loads((root/"sample-finish.json").read_text())
assert finish["status"]=="Pass"
paths=[]
for path in sorted(root.rglob("*")):
    if not path.is_file() or path.name=="archive-manifest.json": continue
    relative=path.relative_to(root)
    if relative.parts[0].startswith("target-") and relative.parts[1:]!=("release","pipeline084"):
        continue
    paths.append(path)
files={str(p.relative_to(root)):dict(bytes=p.stat().st_size,sha256=sha(p)) for p in paths}
for variant in ["baseline","candidate"]:
    assert files[f"target-{variant}/release/pipeline084"]["sha256"]==identity["binaries"][variant]
manifest=root/"archive-manifest.json"
manifest.write_text(json.dumps(dict(format="084-full-raw-v1",frozen_root=str(root),
    baseline=identity["baseline_commit"],candidate=identity["candidate_commit"],schema=identity["schema"],
    files=files,excluded="Only intermediate Cargo target objects/caches; both executed runner binaries retained."),indent=2,sort_keys=True)+"\n")
archive.parent.mkdir(parents=True,exist_ok=True)
with tarfile.open(archive,"w:gz",compresslevel=6) as tar:
    for path in paths+[manifest]:
        tar.add(path,arcname="raw/"+str(path.relative_to(root)),recursive=False)
record=dict(status="Pass",archive_path=str(archive),archive_sha256=sha(archive),archive_bytes=archive.stat().st_size,
    manifest_member="raw/archive-manifest.json",manifest_sha256=sha(manifest),files=len(files),
    uncompressed_file_bytes=sum(p["bytes"] for p in files.values()),baseline=identity["baseline_commit"],
    candidate=identity["candidate_commit"],schema=identity["schema"],window=finish["window"],
    processes=finish["processes"],requests=finish["requests"],
    retained="Exact frozen compiler/runner sources, inputs and edit variants, executed binaries, commands, complete functional artifacts/native outputs, every warmup and retained sample, Build executables, full reports, statistics and failures.",
    limits="Toolchain archives are identified by parent delivery; target object caches excluded. Recorded absolute historical source paths remain provenance, not a claim of location-independent input identity.")
receipt.parent.mkdir(parents=True,exist_ok=True);receipt.write_text(json.dumps(record,indent=2)+"\n")
print(json.dumps(record))

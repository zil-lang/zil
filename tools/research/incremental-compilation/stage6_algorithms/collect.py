#!/usr/bin/env python3
"""Freeze stage6 graph/zit experiments after all exclusive timing windows finish."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile

parser = argparse.ArgumentParser()
parser.add_argument("--repo", type=Path, default=Path.cwd())
parser.add_argument("--research", type=Path, default=Path("/workspace/.zil-env/research"))
parser.add_argument("--out", type=Path, required=True)
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=False)
validation = args.out / "validation"
validation.mkdir()
for source in sorted(args.research.glob("stage6-*.log")):
    if source.name.startswith(("stage6-graph-", "stage6-factor-")):
        shutil.copy2(source, validation / source.name)
for source in args.research.glob("stage6-factor-window.json"):
    shutil.copy2(source, validation / source.name)

directories = [
    "stage6-graph-final", "stage6-graph-final-v2",
    "stage6-algorithms", "stage6-factor-controls",
    "stage6-factor-final", "stage6-factor-final-v2", "stage6-factor-frozen",
    "stage6-factor-benchmark",
]
archives = []
for name in directories:
    source = args.research / name
    if not source.exists():
        raise RuntimeError(f"missing preserved experiment {source}")
    members = []
    for path in sorted(source.rglob("*")):
        if not path.is_file():
            continue
        members.append({"path": str(path.relative_to(source)), "bytes": path.stat().st_size,
                        "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    index = args.out / f"{name}.FILES.json"
    index.write_text(json.dumps(members, indent=2) + "\n")
    archive_path = args.out / f"{name}.tar.gz"
    with tarfile.open(archive_path, "w:gz") as archive:
        for row in members:
            archive.add(source / row["path"], arcname=f"{name}/{row['path']}", recursive=False)
        archive.add(index, arcname=f"{name}/FILES.json", recursive=False)
    archives.append({"path": archive_path.name, "sha256": hashlib.sha256(archive_path.read_bytes()).hexdigest(),
                     "bytes": archive_path.stat().st_size, "members": len(members)})

source_root = args.repo / "tools/research/incremental-compilation/stage6_algorithms"
identities = {}
for path in sorted(source_root.rglob("*")):
    if path.is_file() and "__pycache__" not in path.parts and path.suffix != ".pyc":
        identities[str(path.relative_to(args.repo))] = hashlib.sha256(path.read_bytes()).hexdigest()
for directory in ["063", "071", "076"]:
    for path in sorted((args.repo / f"tools/research/{directory}").glob("*.rs")):
        identities[str(path.relative_to(args.repo))] = hashlib.sha256(path.read_bytes()).hexdigest()
manifest = {
    "head_at_collection": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.repo, text=True).strip(),
    "boundary": "Raw query/scheduler experiments, separate from production CLI measurements. Original failures and superseded functional candidates remain separately identified. No executable/cache artifacts are archived.",
    "measured_tool_snapshot": "7963d69ff993872455c4c5caf6a73ff2a6160fc7",
    "source_identity_scope": "source_sha256 identifies files at collection, including the later failure-recording runner revision. Original formal measurements retain the runner and binary hashes in their own receipts and the 7963d69ff snapshot; they are not relabeled with collection-time hashes.",
    "archives": archives, "source_sha256": identities,
}
(args.out / "IDENTITY.json").write_text(json.dumps(manifest, indent=2) + "\n")
for directory, file in [("stage6-graph-final-v2", "manifest.json"), ("stage6-graph-final-v2", "summary.json"),
                        ("stage6-factor-frozen", "summary.json"), ("stage6-factor-benchmark", "summary.json")]:
    shutil.copy2(args.research / directory / file, args.out / f"{directory}-{file}")
shutil.copy2(args.research / "stage6-algorithms/zit-benchmark-python.json", args.out / "zit-benchmark.json")
shutil.copy2(args.research / "stage6-algorithms/window.json", args.out / "graph-zit-window.json")
print(json.dumps({"out": str(args.out), "archives": archives}, indent=2))

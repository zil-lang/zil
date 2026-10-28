#!/usr/bin/env python3
"""Copy compact, byte-identical review records; never copy executable artifacts."""
import hashlib
import json
from pathlib import Path
import shutil

SOURCE = Path("/workspace/scratch/39be3faa4442/run-logs/190-review")
DEST = Path(__file__).resolve().parent
RUNS = ("author31-01", "independent01", "independent02", "frozen01", "fmt01", "clippy01")
ALLOW = {".json", ".log", ".txt", ".zil", ".wire", ".stdout"}
files = []
omitted = []
for run in RUNS:
    root = SOURCE / run
    for path in sorted(root.rglob("*")):
        if path.is_symlink() or not path.is_file():
            continue
        assert path.resolve().is_relative_to(root.resolve())
        rel = path.relative_to(SOURCE)
        selected = path.suffix in ALLOW and "phases" not in path.relative_to(root).parts
        if run == "author31-01" and "command" not in path.relative_to(root).parts:
            selected = False  # final clean frozen run retains the replay records
        if run == "frozen01" and "review190-by193-native-" in str(rel):
            if path.name in ("typed.txt", "module.ll"):
                selected = True
        content = path.read_bytes()
        row = {
            "original": str(path),
            "relative": str(rel),
            "bytes": len(content),
            "sha256": hashlib.sha256(content).hexdigest(),
        }
        if selected:
            assert not content.startswith((b"\x7fELF", b"MZ")), str(rel)
            destination = DEST / "records" / rel
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
            assert destination.read_bytes() == content
            row["saved"] = str(destination.relative_to(DEST))
            files.append(row)
        else:
            row["reason"] = "derived executable/object or duplicate phase expansion; digest only"
            omitted.append(row)

handoff = SOURCE.parent / "190-review-handoff.json"
content = handoff.read_bytes()
destination = DEST / "records" / "handoff.json"
destination.write_bytes(content)
files.append({"original": str(handoff), "relative": "handoff.json", "bytes": len(content), "sha256": hashlib.sha256(content).hexdigest(), "saved": "records/handoff.json"})

manifest = {
    "status": "Pass",
    "source_root": str(SOURCE),
    "copied_files": len(files),
    "copied_bytes": sum(row["bytes"] for row in files),
    "byte_identical_originals": True,
    "executables_or_objects_copied": 0,
    "files": files,
    "omitted": omitted,
}
(DEST / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps({key: manifest[key] for key in ("status", "copied_files", "copied_bytes", "executables_or_objects_copied")}))

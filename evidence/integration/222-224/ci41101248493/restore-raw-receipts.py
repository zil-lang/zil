#!/usr/bin/env python3
"""Restore this archive from its retained parts, verifying every byte."""
from pathlib import Path
import hashlib,json,os
base=Path(__file__).resolve().parent
manifest=json.loads((base/"artifact-manifest.json").read_text())
parts=manifest["archive_parts"]
assert parts and [p["ordinal"] for p in parts]==list(range(len(parts)))
target=manifest["archive"]
assert Path(target).name==target
dest=base/target
if dest.exists():
    assert dest.stat().st_size==manifest["bytes"]
    assert hashlib.sha256(dest.read_bytes()).hexdigest()==manifest["sha256"]
    print(f"Already restored and verified: {target}")
    raise SystemExit()
tmp=base/(target+".restore-tmp")
assert not tmp.exists(),"Remove an inspected interrupted temporary file before retrying"
whole=hashlib.sha256();total=0
try:
    with tmp.open("xb") as out:
        for row in parts:
            assert Path(row["path"]).name==row["path"]
            part=base/row["path"];assert not part.is_symlink()
            data=part.read_bytes()
            assert len(data)==row["bytes"]
            assert hashlib.sha256(data).hexdigest()==row["sha256"]
            out.write(data);whole.update(data);total+=len(data)
    assert total==manifest["bytes"] and whole.hexdigest()==manifest["sha256"]
    os.replace(tmp,dest)
except BaseException:
    tmp.unlink(missing_ok=True)
    raise
print(f"Restored and verified: {target} ({total} bytes)")

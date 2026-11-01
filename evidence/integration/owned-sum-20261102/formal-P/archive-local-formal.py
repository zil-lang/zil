#!/usr/bin/env python3
"""Pack and verify original local formal evidence; optional native-only release."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys

root=Path(sys.argv[1]).resolve()
output=Path(sys.argv[2]).resolve()
tool=Path('/workspace/zil-core/tools/ci/evidence_transport.py')
spec=importlib.util.spec_from_file_location('transport',tool)
t=importlib.util.module_from_spec(spec);spec.loader.exec_module(t)
t.ROOTS=tuple(sorted(p.name for p in root.iterdir()))
os.environ['GITHUB_SHA']=json.loads((root/'workspace.command.json').read_text())['commit']
index=t.pack(root,output)
verified=t.verify(output)
current,_=t.inventory(root)
assert current==verified['files']
receipt={'origin':'local formal execution, not a CI run','checkout':verified['checkout'],
         'roots':list(t.ROOTS),'files':len(current),'bytes':output.stat().st_size,
         'sha256':t.sha(output),'archive':str(output),
         'every_archive_blob_rehashed':True,'original_inventory_rechecked':True,
         'release_native_requested':len(sys.argv)>3 and sys.argv[3]=='--release-native'}
if receipt['release_native_requested']:
    native=root/'native'
    assert native.is_dir() and native==native.resolve()
    shutil.rmtree(native)
    receipt['native_released_after_verified_archive']=True
(root/'ARCHIVE.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt,indent=2))

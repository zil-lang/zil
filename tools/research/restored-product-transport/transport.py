#!/usr/bin/env python3
"""Round-trip actual limited consumer outputs through unchanged CI transport."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[3]
parser=argparse.ArgumentParser()
parser.add_argument('--runner',type=Path,required=True)
parser.add_argument('--out',type=Path,required=True)
args=parser.parse_args()
runner=args.runner.resolve();out=args.out.resolve()
out.mkdir(parents=True,exist_ok=False)
tool=ROOT/'tools/ci/evidence_transport.py'
commands=[]


def run(arguments,log):
    command=['python3',str(tool),*map(str,arguments)]
    result=subprocess.run(command,capture_output=True,timeout=60)
    (out/(log+'.stdout')).write_bytes(result.stdout)
    (out/(log+'.stderr')).write_bytes(result.stderr)
    commands.append(dict(command=command,exit_code=result.returncode))
    if result.returncode:raise RuntimeError(f'{log}: {result.stderr.decode()}')


archive=out/'native-evidence.tar.zst'
recovered=out/'recovered.tar.zst'
restored=out/'restored'
run(['pack',runner,archive],'pack')
run(['emit',archive],'transport')
run(['recover',out/'transport.stdout',recovered],'recover')
run(['verify',recovered,'--restore',restored],'verify')
spec=importlib.util.spec_from_file_location('impl256_ci_transport',tool)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
original,missing=module.inventory(runner)
copy,copy_missing=module.inventory(restored)
assert original==copy and missing==copy_missing
assert archive.read_bytes()==recovered.read_bytes()
assert all(not any(c in item['path'] for c in '<>:"\\|?*\n\r\0') for item in original)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
receipt=dict(verdict='Pass',evidence_level='lossless actual limited native/source/Wire products; no Cargo/LLVM caches',
             tested_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
             commands=commands,producer_sha256=sha(tool),archive_sha256=sha(archive),archive_bytes=archive.stat().st_size,
             files=len(original),logical_bytes=sum(f['bytes'] for f in original),
             executable_files=sum(bool(f['mode']&0o111) for f in original),missing_optional_roots=missing,
             archive_and_recovered_exact=True,original_and_restored_paths_bytes_sha256_modes_exact=True,
             portable_paths=True,local_transport_only=True,
             transport_log_sha256=sha(out/'transport.stdout'))
(out/'RESULT.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))

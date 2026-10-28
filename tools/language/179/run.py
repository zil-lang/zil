#!/usr/bin/env python3
"""Run one explicit command once, preserving its source identity and full output."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

p = argparse.ArgumentParser()
p.add_argument('--out', required=True)
p.add_argument('command', nargs=argparse.REMAINDER)
a = p.parse_args()
command = a.command[1:] if a.command[:1] == ['--'] else a.command
if not command:
    p.error('a command is required')
root = Path(__file__).resolve().parents[3]
out = Path(a.out).resolve()
out.mkdir(parents=True, exist_ok=False)
def git(*args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()
record = {'command': command, 'cwd': str(root), 'source_commit': git('rev-parse', 'HEAD'), 'dirty': git('status', '--porcelain'), 'spec_sha256': hashlib.sha256((root/'_zil_design.md').read_bytes()).hexdigest(), 'environment': {k: os.environ.get(k) for k in ['CARGO_TARGET_DIR', 'CARGO_INCREMENTAL', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG', 'ZIL_CLANG', 'ZIL_TEST_REPORT_DIR']}}
started = time.time()
with (out/'output.log').open('wb') as log:
    result = subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, check=False)
record.update(exit_code=result.returncode, elapsed_seconds=time.time()-started, status='Pass' if result.returncode == 0 else 'Fail')
(out/'command.json').write_text(json.dumps(record, indent=2)+'\n')
print(json.dumps(record, indent=2))
print((out/'output.log').read_text(errors='replace')[-18000:])
sys.exit(result.returncode)

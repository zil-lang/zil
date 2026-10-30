#!/bin/bash
set -eu
. /workspace/scratch/ac5966c4c370/toolchains/env.sh
python3 - <<'PYSCRIPT'
import json,subprocess,pathlib
p=pathlib.Path('/tmp/cp13-root-review')
r=subprocess.run(json.loads((p/'commands.json').read_text())['commands'][0],stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
(p/'build.log').write_bytes(r.stdout)
print(r.stdout.decode())
raise SystemExit(r.returncode)
PYSCRIPT
/tmp/cp13-root-review/review --nocapture --test-threads=1 > /tmp/cp13-root-review/tests.log 2>&1
cat /tmp/cp13-root-review/tests.log

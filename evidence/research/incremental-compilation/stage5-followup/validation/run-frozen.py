from pathlib import Path
import datetime
import json
import subprocess
import zoneinfo

root = Path('/workspace/zil')
out = Path('/workspace/.zil-env/research/stage5-followup')
commit = '59d8753dc23f0ddfcddbaf5780e3057f3c60b9e9'
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip() == commit
base = ['--zil', '/workspace/.zil-env/research/target-stage5-followup-final/release/zil',
        '--rustc', subprocess.check_output(['rustup', 'which', 'rustc'], text=True).strip(), '--blocks', '6']
cases = ['large-128', 'large-512', 'large-2048', 'large-4096', 'large-8192',
         'even-64x64', 'large-skew-8', 'body-16x16', 'types-32x16']
base += sum((['--case', case] for case in cases), [])
jobs = [
    ('followup-default', ['--mode', 'O0', '--mode', 'O2']),
    ('followup-closed', ['--mode', 'O2', '--closed-executable']),
    ('followup-isolation', ['--mode', 'O0', '--mode', 'O2', '--reference-zil',
                            '/workspace/.zil-env/research/target-stage5/release/zil']),
]
now = lambda: datetime.datetime.now(zoneinfo.ZoneInfo('Asia/Shanghai')).isoformat()
records = []
for name, extra in jobs:
    cmd = ['python3', 'tools/research/incremental-compilation/distribution_bench.py',
           *base, *extra, '--out', str(out / name)]
    row = {'window': name, 'command': cmd, 'production_commit': commit, 'started': now()}
    print('START', name, row['started'], flush=True)
    with (out / (name + '.log')).open('xb') as log:
        run = subprocess.run(cmd, cwd=root, stdout=log, stderr=subprocess.STDOUT)
    row.update(exit=run.returncode, finished=now())
    records.append(row)
    (out / 'frozen-execution.json').write_text(json.dumps(records, indent=2) + '\n')
    print('FINISH', name, row['exit'], row['finished'], flush=True)
    if run.returncode != 0:
        raise SystemExit(run.returncode)

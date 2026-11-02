import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import time

p = argparse.ArgumentParser()
p.add_argument('--source', required=True)
p.add_argument('--run', required=True)
p.add_argument('--known-gap', action='append', default=[])
args = p.parse_args()
repo = Path('/workspace/zil-251')
base = Path('/workspace/.zil-env/forward')
out = base / args.run
out.mkdir(exist_ok=False)
def identity():
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip() == args.source
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo, text=True).strip()
identity()
os.environ['GITHUB_SHA'] = args.source
(out/'toolchain.txt').write_bytes(subprocess.check_output(['rustc', '-vV'], cwd=repo))
with (out/'toolchain.txt').open('ab') as stream:
    stream.write(subprocess.check_output([os.environ.get('ZIL_CLANG', 'clang'), '--version'], cwd=repo))
spec = importlib.util.spec_from_file_location('transport', repo/'tools/ci/evidence_transport.py')
transport = importlib.util.module_from_spec(spec)
spec.loader.exec_module(transport)
report = {'source_commit': args.source, 'scope': 'formal common-candidate gates',
          'known_feature_gaps_not_closed_by_regression_pass': args.known_gap,
          'profiles': {'debug': 0, 'strip': 'symbols', 'incremental': False, 'jobs': 2},
          'checks': [], 'archives': [], 'status': 'Running'}
def save():
    (out/'RESULT.json').write_text(json.dumps(report, indent=2)+'\n')
save()
stages = [
    ('fmt', ['cargo', 'fmt', '--all', '--', '--check'], 120, '251'),
    ('workspace', ['cargo', 'test', '--offline', '--locked', '--workspace', '--no-fail-fast'], 1800, '251'),
    ('clippy', ['cargo', 'clippy', '--offline', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings'], 600, '251'),
    ('reference', ['cargo', 'test', '--offline', '--locked', '--workspace', '--no-default-features', '--no-fail-fast'], 1800, '251-reference'),
    ('text-reference', ['cargo', 'test', '--offline', '--locked', '-p', 'zil-llvm-api', '--features', 'text-reference'], 300, '251'),
    ('migration', ['python3', 'tools/llvm/test_validate_migration.py'], 300, '251'),
    ('transport', ['python3', 'tools/ci/test_evidence_transport.py'], 300, '251'),
]
for name, command, timeout, target in stages:
    identity()
    stage = out/name
    (stage/'receipts').mkdir(parents=True)
    log = stage/'receipts'/'output.log'
    env = dict(os.environ, CARGO_TARGET_DIR='/workspace/.zil-env/target-'+target,
               CARGO_PROFILE_TEST_STRIP='symbols', CARGO_PROFILE_DEV_STRIP='symbols',
               ZIL_TEST_REPORT_DIR=str(stage/'native-evidence'), GITHUB_SHA=args.source)
    start = time.monotonic()
    expired = False
    with log.open('w') as stream:
        process = subprocess.Popen(command, cwd=repo, env=env, stdout=stream,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            expired = True
            os.killpg(process.pid, signal.SIGTERM)
            try:
                code = process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait()
    rows = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;', log.read_text(errors='replace'))
    totals = dict(zip(['passed', 'failed', 'ignored', 'measured', 'filtered'], [sum(int(row[i]) for row in rows) for i in range(5)]))
    item = dict(name=name, command=command, exit_code=code, timeout=expired,
                elapsed_seconds=round(time.monotonic()-start, 3),
                log_sha256=transport.sha(log), test_summaries=len(rows), totals=totals)
    item['status'] = 'Pass' if code == 0 and not expired else 'Fail'
    if name in ['workspace', 'reference', 'text-reference'] and (not rows or any(totals[k] for k in ['failed','ignored','filtered'])):
        item['status'] = 'Fail'
    report['checks'].append(item)
    (stage/'receipts'/'check.json').write_text(json.dumps(dict(source_commit=args.source, **item), indent=2)+'\n')
    save()
    print(json.dumps(item), flush=True)
    if (stage/'native-evidence').exists():
        archive = out/(name+'.tar.zst')
        index = transport.pack(stage, archive)
        recovered_index = transport.verify(archive)
        actual, missing = transport.inventory(stage)
        assert index == recovered_index and index['files'] == actual and index['missing_roots'] == missing
        summary = dict(phase=name, source_commit=args.source, status='Pass',
                       archive=str(archive), archive_bytes=archive.stat().st_size,
                       archive_sha256=transport.sha(archive), files=len(actual),
                       logical_bytes=sum(x['bytes'] for x in actual),
                       verification='All indexed archive blobs hashed; exact original path/size/mode/SHA256 set rechecked before release')
        (out/(name+'-archive.json')).write_text(json.dumps(summary, indent=2)+'\n')
        report['archives'].append(summary)
        save()
        shutil.rmtree(stage/'native-evidence')
        print(json.dumps(summary), flush=True)
    if item['status'] != 'Pass':
        report['status'] = 'Fail'
        save()
        raise SystemExit(1)
identity()
report['status'] = 'Pass'
save()

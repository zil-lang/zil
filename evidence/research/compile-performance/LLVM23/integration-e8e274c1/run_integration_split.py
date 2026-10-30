"""Run unchanged workspace tests in groups so test binaries fit the task disk."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path('/workspace/scratch/ac5966c4c370')
REPO = ROOT / 'zil-research'
OUT = ROOT / 'integration-e8e274c1-split'
TARGET = Path('/tmp/zil-compile-integration-target')
OUT.mkdir(exist_ok=False)
env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET), CARGO_PROFILE_TEST_DEBUG='0', CARGO_INCREMENTAL='0')
meta = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1', '--offline'], cwd=REPO, env=env))
driver = next(p for p in meta['packages'] if p['name'] == 'zil-driver')
source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip()
commands = [('workspace-except-driver', ['--workspace', '--exclude', 'zil-driver'], None)]
commands.append(('driver-lib', ['-p', 'zil-driver', '--lib'], 'zil_driver'))
commands.extend(('driver-test-' + t['name'], ['-p', 'zil-driver', '--test', t['name']], t['name']) for t in driver['targets'] if t['kind'] == ['test'])
commands.extend([('driver-doc', ['-p', 'zil-driver', '--doc'], None), ('driver-examples', ['-p', 'zil-driver', '--examples'], None)])
record = {'source': source, 'reason': 'Initial workspace compile exhausted disk; keep exact source and test profile, execute all targets in bounded groups.', 'commands': [], 'pass': False}
for index, (label, args, cleanup_name) in enumerate(commands):
    command = ['cargo', 'test', *args, '--locked', '--offline', '-j', '2']
    log = OUT / (label + '.log')
    with log.open('xb') as stream:
        result = subprocess.run(command, cwd=REPO, env=env, stdout=stream, stderr=subprocess.STDOUT)
    text = log.read_text(errors='replace')
    counts = [tuple(map(int, m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out', text)]
    row = {'label': label, 'argv': command, 'exit': result.returncode, 'log': log.name, 'sha256': hashlib.sha256(log.read_bytes()).hexdigest(), 'test_summaries': counts}
    record['commands'].append(row)
    (OUT / 'RESULT.json').write_text(json.dumps(record, indent=2) + '\n')
    print(f'{index+1}/{len(commands)} {label} exit={result.returncode}', flush=True)
    if result.returncode:
        sys.exit(result.returncode)
    if cleanup_name:
        prefix = cleanup_name.replace('-', '_')
        for artifact in (TARGET / 'debug' / 'deps').iterdir():
            if re.fullmatch(re.escape(prefix) + r'-[0-9a-f]+', artifact.name) and artifact.is_file():
                artifact.unlink()
record['pass'] = True
record['totals'] = [sum(summary[i] for c in record['commands'] for summary in c['test_summaries']) for i in range(5)]
(OUT / 'RESULT.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'pass': True, 'totals': record['totals'], 'command_count': len(commands)}), flush=True)

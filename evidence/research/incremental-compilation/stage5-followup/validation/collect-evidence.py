from pathlib import Path
import datetime
import hashlib
import json
import platform
import shutil
import subprocess
import zoneinfo

root = Path('/workspace/zil')
source = Path('/workspace/.zil-env/research/stage5-followup')
target = root / 'evidence/research/incremental-compilation/stage5-followup'
commit = '59d8753dc23f0ddfcddbaf5780e3057f3c60b9e9'
windows = ['followup-default', 'followup-closed', 'followup-isolation']
execution = json.loads((source / 'frozen-execution.json').read_text())
assert [row['window'] for row in execution] == windows and all(row['exit'] == 0 for row in execution)
assert subprocess.run(['git', 'diff', '--quiet', commit, '--', 'crates'], cwd=root).returncode == 0
target.mkdir(parents=True, exist_ok=False)
validation = target / 'validation'
validation.mkdir()
for path in source.iterdir():
    if path.is_file() and (path.suffix in {'.log', '.json', '.py', '.rs', '.md'} or path.name.endswith('-oracle.tar.gz')):
        shutil.copy2(path, validation / path.name)
shutil.copy2(source / 'analysis/analysis.json', target / 'analysis.json')
shutil.copy2(source / 'analysis/DISTRIBUTION-TABLES.md', root / 'docs/research/incremental-compilation/PREFLIGHT-FOLLOWUP-TABLES.md')
freeze = root / 'tools/research/incremental-compilation/freeze_evidence.py'
for name in windows:
    dest = target / name
    dest.mkdir()
    for file in ['manifest.json', 'summary.json']:
        shutil.copy2(source / name / file, dest / file)
    subprocess.run(['python3', str(freeze), '--root', str(source), '--out', str(dest / 'raw.tar.gz'), name], check=True)
subprocess.run(['python3', str(freeze), '--root', str(source), '--out', str(target / 'controls/raw.tar.gz'),
                'sanity-generated', 'sanity-real', 'final-sanity-generated', 'final-sanity-real'], check=True)

def capture(cmd):
    return subprocess.check_output(cmd, cwd=root, text=True).strip()

def identity(path):
    path = Path(path)
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'bytes': path.stat().st_size}

environment = {
    'collected_at': datetime.datetime.now(zoneinfo.ZoneInfo('Asia/Shanghai')).isoformat(),
    'production_commit': commit,
    'baseline_production_commit': '801933a9e8f060d44b8305786d01399527aa6137',
    'benchmark_tools_commit': '283c661bcb18978fe048770dace4994ade1910d2',
    'production_crates_tree': capture(['git', 'rev-parse', commit + ':crates']),
    'rustc': capture(['rustc', '-vV']), 'clang': capture(['clang', '--version']),
    'platform': platform.platform(), 'cpu': json.loads(capture(['lscpu', '-J'])),
    'measurement_affinity': json.loads((source / 'followup-default/manifest.json').read_text())['cpus'],
    'binaries': {name: identity(path) for name, path in {
        'zil': '/workspace/.zil-env/research/target-stage5-followup-final/release/zil',
        'baseline': '/workspace/.zil-env/research/target-stage5/release/zil',
        'superseded_05e_controls_only': '/workspace/.zil-env/research/target-stage5-followup/release/zil',
        'rust': capture(['rustup', 'which', 'rustc']),
    }.items()},
    'boundary': 'Three exclusive six-block windows only. Final sanity204 requests and14 reference builds are correctness controls; 05e controls are separately superseded, never final performance evidence. No ELF/cache work products archived.'
}
(validation / 'environment.json').write_text(json.dumps(environment, indent=2) + '\n')
print(json.dumps({'target': str(target), 'windows': windows, 'done': True}))

from pathlib import Path
import datetime
import hashlib
import json
import os
import platform
import shutil
import subprocess
import zoneinfo

root = Path('/workspace/zil')
source = Path('/workspace/.zil-env/research/stage5')
target = root / 'evidence/research/incremental-compilation/stage5'
windows = ['generated-default', 'generated-closed', 'implementation-ablation', 'real-default', 'real-closed']
execution = json.loads((source / 'frozen-execution.json').read_text())
assert [x['window'] for x in execution] == windows and all(x['exit'] == 0 for x in execution)
assert subprocess.run(['git', 'diff', '--quiet', '801933a9e8f060d44b8305786d01399527aa6137', '--', 'crates'], cwd=root).returncode == 0
target.mkdir(parents=True, exist_ok=False)
validation = target / 'validation'
validation.mkdir()
for path in source.iterdir():
    if path.is_file() and (path.suffix in {'.log', '.json', '.py', '.rs', '.md', '.ts', '.mjs'} or path.name == 'review-dominance-oracle.tar.gz'):
        shutil.copy2(path, validation / path.name)
shutil.copy2(source / 'analysis/analysis.json', target / 'analysis.json')
shutil.copy2(source / 'analysis/DISTRIBUTION-TABLES.md', root / 'docs/research/incremental-compilation/DISTRIBUTION-TABLES.md')
freeze = root / 'tools/research/incremental-compilation/freeze_evidence.py'
for window in windows:
    out = target / window
    out.mkdir()
    for name in ['manifest.json', 'summary.json']:
        shutil.copy2(source / window / name, out / name)
    subprocess.run(['python3', str(freeze), '--root', str(source), '--out', str(out / 'raw.tar.gz'), window], check=True)
diagnostics = ['shape-smoke', 'shape-smoke-corrected', 'incremental-diagnostic',
               'final-shape-preflight', 'closed-and-large-preflight', 'real-preflight',
               'app-corrected-preflight', 'new-types-preflight', 'ablation-protocol-smoke',
               'real-closed-preflight', 'review-protocol', 'source-control']
subprocess.run(['python3', str(freeze), '--root', str(source), '--out', str(target / 'diagnostics/raw.tar.gz'), *diagnostics], check=True)

def capture(cmd):
    return subprocess.check_output(cmd, cwd=root, text=True).strip()

def identity(path):
    path = Path(path)
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'bytes': path.stat().st_size}

environment = {
    'collected_at': datetime.datetime.now(zoneinfo.ZoneInfo('Asia/Shanghai')).isoformat(),
    'production_commit': '801933a9e8f060d44b8305786d01399527aa6137',
    'measurement_tools_commit': '283c661bcb18978fe048770dace4994ade1910d2',
    'baseline_delivery_commit': '12fc97bde3a89de450c0eabebbcf11d512123018',
    'baseline_production_commit': '9fe5f55e64c7d537b9d585d3d40e42c9e248308e',
    'production_crates_tree': capture(['git', 'rev-parse', '801933a9e:crates']),
    'baseline_crates_tree': capture(['git', 'rev-parse', '9fe5f55e:crates']),
    'rustc': capture(['rustc', '-vV']), 'clang': capture(['clang', '--version']),
    'platform': platform.platform(), 'cpu': json.loads(capture(['lscpu', '-J'])),
    'collector_affinity': sorted(os.sched_getaffinity(0)),
    'measurement_affinity': json.loads((source / 'generated-default/manifest.json').read_text())['cpus'],
    'binaries': {name: identity(path) for name, path in {
        'zil': '/workspace/.zil-env/research/target-stage5/release/zil',
        'baseline': '/workspace/.zil-env/research/target-stage4/release/zil',
        'rust': capture(['rustup', 'which', 'rustc']),
    }.items()},
    'zit_branch': capture(['git', 'rev-parse', 'origin/task/cp25-reactive-session']),
    'zit_original': identity(source / 'zit.ts'),
    'closed_port_source': 'e7b492c6e04128e6106bbf29c0d6c7b381556d77',
    'boundary': 'No ELF/cache work products archived. Corpus, raw outcomes and tool/source/binary hashes retained. Frozen windows finished before postprocessing/archiving.'
}
(validation / 'environment.json').write_text(json.dumps(environment, indent=2) + '\n')
print(json.dumps({'target': str(target), 'windows': windows, 'done': True}))

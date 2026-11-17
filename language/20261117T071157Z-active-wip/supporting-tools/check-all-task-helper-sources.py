#!/usr/bin/env python3
"""Enumerate every existing native-Task helper case and collect all Source failures.

This is a diagnostic census, not Alpha/native acceptance. Rust test helpers are
copied verbatim; only include_str paths are made absolute for the external tool.
"""
import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

from lossless_phase_logs import compress_phases

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--repo', required=True, type=Path)
p.add_argument('--cli', required=True, type=Path)
p.add_argument('--build-receipt', required=True, type=Path)
p.add_argument('--out', required=True, type=Path)
p.add_argument('--work-profile', choices=['standard', 'alpha'], default='standard')
p.add_argument('--jobs', type=int, default=2)
a = p.parse_args()
sha = lambda data: hashlib.sha256(data).hexdigest()
identity = json.loads(a.build_receipt.read_text())
candidate = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=a.repo, text=True).strip()
assert identity['status'] == 'Pass' and identity['source_commit'] == candidate
a.out.mkdir(parents=True, exist_ok=False)
cli = a.out / 'zil'
shutil.copy2(a.cli, cli)
assert sha(cli.read_bytes()) == identity['cli_sha256'] == sha(a.cli.read_bytes())
original_path = a.repo / 'crates/zil-driver/tests/task_owner_transport.rs'
original = original_path.read_text()
start = original.index('fn cancel_drive(text:')
end = original.index('#[test]\nfn consuming_callable_returns_', start)
helper = original[start:end]
fixture = a.repo / 'tests/language-readiness/task-owner-transport/returned-task-fnonce.zil'
code = 'const SOURCE: &str = include_str!(' + json.dumps(str(fixture)) + ');\n'
code += helper.replace('../../../tests/', str(a.repo / 'tests') + '/')
code += r'''
fn main() {
    let out = std::path::PathBuf::from(std::env::args_os().nth(1).unwrap());
    let all = vec![("basic", SOURCE.to_owned()), ("closed-owner", closed_owner())]
        .into_iter().chain(cases()).chain(returned_frame_cases())
        .chain(mutable_frame_cases()).chain(static_frame_cases());
    for (name, text) in all {
        let dir = out.join(name);
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("source.zil"), text).unwrap();
        println!("{name}");
    }
}
'''
rust_source = a.out / 'enumerator.rs'
rust_source.write_text(code)
enumerator = a.out / 'enumerator'
command = ['rustc', '--edition=2024', str(rust_source), '-o', str(enumerator)]
build = subprocess.run(command, capture_output=True, timeout=30)
(a.out / 'enumerator-build.stderr').write_bytes(build.stderr)
assert build.returncode == 0, build.stderr.decode(errors='replace')
names = subprocess.check_output([str(enumerator), str(a.out)], text=True).splitlines()
assert len(names) >= 30 and len(names) == len(set(names))

def check(name):
    d = a.out / name
    source = d / 'source.zil'
    command = [str(cli), 'check', str(source), '--work-profile', a.work_profile,
               '--emit-dir', str(d / 'phases'), '--json']
    result = subprocess.run(command, cwd=a.repo, capture_output=True, timeout=180)
    (d / 'stdout.json').write_bytes(result.stdout)
    (d / 'stderr.txt').write_bytes(result.stderr)
    report = json.loads(result.stdout)
    row = {'case': name, 'source_sha256': sha(source.read_bytes()),
           'exit_code': result.returncode, 'status': report.get('status'),
           'phase': report.get('phase'), 'verification_runs': report.get('verification_runs'),
           'diagnostics': report.get('diagnostics'), 'command': command}
    (d / 'receipt.json').write_text(json.dumps(row, indent=2) + '\n')
    compress_phases(d)
    print(name, row['status'], row['phase'], flush=True)
    return row

with concurrent.futures.ThreadPoolExecutor(max_workers=a.jobs) as pool:
    rows = list(pool.map(check, names))
success = sum(row['status'] == 'Success' and row['verification_runs'] == 1 for row in rows)
receipt = {'candidate': candidate, 'cli_sha256': identity['cli_sha256'],
           'scope': 'All exact existing Task helper Source cases; diagnostic only, not native acceptance',
           'original_helper_file_sha256': sha(original.encode()),
           'exact_helper_excerpt_sha256': sha(helper.encode()),
           'work_profile': a.work_profile, 'selected': len(rows), 'source_verified': success,
           'rows': rows}
(a.out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print('CENSUS_COMPLETE', len(rows), success, str(a.out / 'receipt.json'), flush=True)

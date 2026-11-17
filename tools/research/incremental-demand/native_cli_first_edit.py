#!/usr/bin/env python3
"""Ordinary CLI cold -> exit -> edit -> first build, with retained caches.

Companion to the complete direct-edit matrix. Reuses its pinned commands and
original source recipes, removes only the ZIL resident protocol options, and
times process launch through exit for all three arms. No profile instrumentation.
"""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import types

import closed_project_matrix as matrix


def ordinary(argv):
    result, index = [], 0
    while index < len(argv):
        flag = argv[index]
        if flag == '--resident':
            index += 1
        elif flag in ('--resident-capacity', '--resident-reset-after'):
            index += 2
        else:
            result.append(flag)
            index += 1
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--window', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--blocks', type=int, default=6)
    args = parser.parse_args()
    parent, out = args.window.resolve(), args.out.absolute()
    require, rc = matrix.require, matrix.rc
    require(args.blocks >= 2 and args.blocks % 2 == 0, 'an even number of paired blocks required')
    completion = json.loads((parent / 'completion.json').read_text())
    require(completion['execution_complete'] and completion['candidate_rust_accepted'],
            'complete functional/runtime parent matrix required')
    manifest = json.loads((parent / 'manifest.json').read_text())
    pins = manifest['pins']
    repo = Path(pins['candidate']['worktree'])
    require(not subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo), 'freeze source')
    tool_source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
    require(not subprocess.check_output(['git', 'diff', '--name-only', pins['candidate']['source'],
            tool_source, '--', 'crates', 'Cargo.toml', 'Cargo.lock', '.cargo'], cwd=repo), 'candidate source binding')
    installed = Path(__file__).resolve()
    require(installed.read_bytes() == subprocess.check_output(
        ['git', 'show', tool_source + ':' + str(installed.relative_to(repo))], cwd=repo), 'frozen CLI tool')
    for arm in ('baseline', 'candidate'):
        require(matrix.matched.file_hash(pins[arm]['binary']) == pins[arm]['binary_sha256'], 'binary pin')
        require(matrix.matched.file_hash(Path(pins[arm]['llvm_library_path']) / 'libLLVM.so.23.1')
                == pins[arm]['llvm_dso_sha256'], 'LLVM DSO pin')
    for tool in manifest['tools'].values():
        require(matrix.matched.file_hash(tool['actual_path']) == tool['actual_sha256'], 'tool pin')
    require(not out.exists(), 'new output required')
    out.mkdir(parents=True)
    env = matrix.large.clean_environment()
    env.update(CARGO_HOME=str(parent / 'cargo-home'), RUSTC=manifest['tools']['rustc']['actual_path'],
               CARGO_INCREMENTAL='1', CARGO_BUILD_JOBS='1', CARGO_TERM_COLOR='never')
    fixture_args = types.SimpleNamespace(preset='5105', modules=100, shape='short', graph='wide')
    workloads = rc.load_workloads(repo)
    history = matrix.history(8)
    requests = [history[0], next(r for r in history if r['name'] == 'private-001')]
    rows = []
    for block in range(args.blocks):
        prior = parent / f'workflow/5105/block-{block % 2:02d}'
        base = out / f'block-{block:02d}'
        for arm in ('baseline', 'candidate'):
            (base / 'user' / arm).mkdir(parents=True)
        for request in requests:
            files, metadata = matrix.fixture(fixture_args, workloads, request)
            for lang in ('zil', 'rust'):
                rc.write_files(base / 'sources' / lang, files[lang], '.zil' if lang == 'zil' else '.rs')
            project = base / 'sources/rust'
            for name in ('Cargo.toml', 'Cargo.lock'):
                data = (prior / 'sources/rust' / name).read_bytes()
                if not (project / name).exists() or (project / name).read_bytes() != data:
                    (project / name).write_bytes(data)
            oracle = matrix.Oracle(fixture_args, workloads, request)
            expected_files = {str(base / 'sources/zil' / name) for name in files['zil']}
            order = ('baseline', 'candidate', 'rust') if block % 2 == 0 else ('rust', 'candidate', 'baseline')
            for arm in order:
                path = (prior / 'requests/002-private-001/rust/command.json' if arm == 'rust'
                        else prior / f'resident/{arm}/1/command.json')
                command = json.loads(path.read_text())
                argv = [x.replace(str(prior), str(base)) for x in ordinary(command['argv'])]
                cwd = project if arm == 'rust' else Path(command['cwd'])
                current_env = env.copy()
                for key, value in command.get('environment', {}).items():
                    if value is None:
                        current_env.pop(key, None)
                    else:
                        current_env[key] = value
                directory = base / request['name'] / arm
                before = matrix.large.disk_snapshot(matrix.large.disk_roots(base / 'user'))
                result = rc.run_process(argv, cwd, directory, 300, 5, env=current_env)
                after = matrix.large.disk_snapshot(matrix.large.disk_roots(base / 'user'))
                require(not result['timed_out'], 'compiler timeout')
                if arm == 'rust':
                    messages = [json.loads(line) for line in (directory / 'stdout.bin').read_text().splitlines()]
                    products = [m for m in messages if m.get('reason') == 'compiler-artifact']
                    require(result['returncode'] == 0 and products and any(not m['fresh'] for m in products),
                            'cold and edited Cargo requests must execute rustc')
                    program = Path(next(m['executable'] for m in products if m.get('executable')))
                    scope = {'rustc_invoked': True, 'artifacts_fresh': [m['fresh'] for m in products]}
                else:
                    report = json.loads((directory / 'stdout.bin').read_text())
                    require(set(report['source_files'].values()) == expected_files, 'complete original source map')
                    allowed_limit = arm == 'baseline' and report['status'] == 'ResourceLimit'
                    require(allowed_limit or result['returncode'] == 0 and report['status'] == 'Success'
                            and report['verification_runs'] == 1 and not report['diagnostics'], 'current Verify/build')
                    scope = {'status': report['status'], 'verification_runs': report['verification_runs'],
                             'source_files': len(expected_files)}
                    program = None if allowed_limit else Path(report['artifacts']['executable'])
                if program is not None:
                    for index, data in enumerate((b'', b'A', b'\xff')):
                        run = subprocess.run([str(program)], input=data, capture_output=True, timeout=5, env=current_env)
                        require({'exit': run.returncode, 'stdout_hex': run.stdout.hex(), 'stderr_hex': run.stderr.hex()}
                                == oracle.oracle('scalar', data, ''), f'native output control {index}')
                rows.append({'block': block, 'request': request['name'], 'arm': arm, 'measurement': result,
                             'disk_before': before, 'disk_after': after,
                             'scope': scope, 'source_equivalence': metadata})
                rc.save(out / 'rows.json', rows)
                print(json.dumps({'block': block, 'request': request['name'], 'arm': arm,
                                  'wall_ms': result['wall_ms'], 'scope': scope}), flush=True)
    medians = {r['name']: {arm: statistics.median(row['measurement']['wall_ms'] for row in rows
               if row['request'] == r['name'] and row['arm'] == arm) for arm in matrix.matched.ARMS} for r in requests}
    paired = [{arm: next(row['measurement']['wall_ms'] for row in rows
               if row['block'] == block and row['request'] == 'private-001' and row['arm'] == arm)
               for arm in ('candidate', 'rust')} for block in range(args.blocks)]
    rc.save(out / 'summary.json', {'execution_complete': True, 'rows': len(rows), 'tool_source': tool_source,
            'blocks': args.blocks, 'first_edit_pairs_ms': paired,
            'candidate_faster_in_every_first_edit_pair': all(p['candidate'] < p['rust'] for p in paired),
            'candidate_source': pins['candidate']['source'], 'candidate_binary_sha256': pins['candidate']['binary_sha256'],
            'actual_main': pins['baseline']['source'], 'parent': str(parent), 'median_ms': medians,
            'clock': 'ordinary CLI launch through process exit; default session capacity; no diagnostic flags',
            'profile': 'O2/CGU16/compiler jobs1; original budgets; explicit research options from parent',
            'history': 'empty per-block user caches -> cold -> process exit -> edit -> fresh process first build',
            'full_runtime_gate': 'parent matrix; three output controls per successful CLI build only'})


if __name__ == '__main__':
    main()

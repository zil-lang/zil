#!/usr/bin/env python3
"""Four retained-cache CLI controls after one complete check history.

All three arms include process launch through exit. Fresh and timestamp-only
forced-rustc requests stay separate. No compiler flags, source bytes or caches
are cleared; this is a check/dev control, not an O2 or equal-obligation claim.
"""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess

import large_project_matrix as large
import native_link_matched_measure as matched
import rust_compare as rc


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--window', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    window, out = args.window.resolve(), args.out.absolute()
    large.require(not out.exists(), 'new output required')
    manifest = json.loads((window/'manifest.json').read_text())
    large.require(json.loads((window/'completion.json').read_text())['valid'], 'complete parent history')
    pins = manifest['pins']
    repo = Path(pins['candidate']['worktree'])
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
    large.require(not subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo), 'freeze tool source')
    for arm in ['baseline', 'candidate']:
        large.require(matched.file_hash(pins[arm]['binary']) == pins[arm]['binary_sha256'], 'binary identity')
        large.require(matched.file_hash(Path(pins[arm]['llvm_library_path'])/'libLLVM.so.23.1')
                      == pins[arm]['llvm_dso_sha256'], 'LLVM identity')
    for tool in manifest['tools'].values():
        large.require(matched.file_hash(tool['actual_path']) == tool['actual_sha256'], 'tool identity')
    out.mkdir(parents=True)
    env = large.clean_environment()
    env.update(CARGO_HOME=str(window/'cargo-home'), RUSTC=manifest['tools']['rustc']['actual_path'],
               CARGO_INCREMENTAL='1', CARGO_BUILD_JOBS='1', CARGO_TERM_COLOR='never')
    rows = []
    for block in range(2):
        root = window/f'sources/block-{block:02d}'
        def identity():
            return {str(path.relative_to(root)): matched.file_hash(path)
                    for path in sorted(root.rglob('*')) if path.is_file()}
        original = identity()
        expected = {str(path.resolve()) for path in (root/'zil').rglob('*.zil')}
        commands = {}
        for arm in ['baseline', 'candidate']:
            command = json.loads((window/f'resident/block-{block:02d}/{arm}/2/command.json').read_text())
            argv, filtered, index = command['argv'], [], 0
            while index < len(argv):
                flag = argv[index]
                if flag in ['--resident-capacity', '--resident-reset-after']:
                    index += 2
                elif flag == '--resident':
                    index += 1
                else:
                    filtered.append(flag)
                    index += 1
            command['argv'] = filtered
            commands[arm] = command
        commands['rust'] = json.loads((window/f'requests/block-{block:02d}/017-compiler-restart/rust/command.json').read_text())
        for index, request in enumerate(['fresh', 'forced-rustc']):
            if request == 'forced-rustc':
                for path in [root/'zil/main.zil', root/'rust/src/main.rs']:
                    os.utime(path, None)
            large.require(identity() == original, 'identical source bytes and manifest')
            order = ['baseline', 'candidate', 'rust'] if (block+index) % 2 == 0 else ['rust', 'candidate', 'baseline']
            for arm in order:
                command = commands[arm]
                current_env = env.copy()
                for key, value in command.get('environment', {}).items():
                    if value is None:
                        current_env.pop(key, None)
                    else:
                        current_env[key] = value
                directory = out/f'block-{block:02d}'/request/arm
                result = rc.run_process(command['argv'], Path(command['cwd']), directory, 120, 5, env=current_env)
                large.require(result['returncode'] == 0 and not result['timed_out'], 'CLI success')
                if arm == 'rust':
                    scope = large.rust_compiler_scope(directory)
                    large.require(scope['rustc_invoked'] == (request == 'forced-rustc')
                                  and scope['crate_artifacts_fresh'] == [request == 'fresh'], 'actual rustc scope')
                else:
                    report = json.loads((directory/'stdout.bin').read_text())
                    large.require(report['status'] == 'Success' and report['verification_runs'] == 1
                                  and report['diagnostics'] == []
                                  and set(report['source_files'].values()) == expected, 'complete current Verify and source closure')
                    scope = {'verification_runs': report['verification_runs'], 'source_files': len(expected)}
                rows.append({'block': block, 'request': request, 'arm': arm, 'measurement': result, 'scope': scope})
                rc.save(out/'rows.json', rows)
            large.require(identity() == original, 'source unchanged through CLI control')
    comparisons = []
    for request in ['fresh', 'forced-rustc']:
        samples = {arm: [row['measurement']['wall_ms'] for row in rows
                        if row['request'] == request and row['arm'] == arm] for arm in large.ARMS}
        comparisons.append({'request': request, 'wall_ms_by_block': samples,
                            'median_ms': {arm: statistics.median(values) for arm, values in samples.items()}})
    result = {'valid': True, 'rows': len(rows), 'quality_cells': 4, 'tool_source': source,
              'tool_sha256': matched.file_hash(Path(__file__)), 'candidate_source': pins['candidate']['source'],
              'candidate_binary_sha256': pins['candidate']['binary_sha256'], 'actual_main': pins['baseline']['source'],
              'parent_window': str(window), 'clock': 'Popen through process exit for all arms; includes startup/compile/teardown',
              'profile': 'check/dev default opt0, CGU16/jobs1; complete retained post-history caches; identical bytes',
              'same_compiler_obligations': False, 'comparisons': comparisons}
    rc.save(out/'summary.json', result)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Freeze protocol and source snapshots only. Never launches a compiler."""
import argparse
import datetime
import json
import subprocess
from pathlib import Path
from fixture import generate, generate_control, sha, changed_names


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo', type=Path, default=Path('/workspace/scratch/abf690ce880c/zil'))
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--files', type=int, nargs='+', default=[64, 256], choices=[64, 256, 1024])
    p.add_argument('--edits', type=int, nargs='+', default=[1, 4, 16, 64], choices=[1, 4, 16, 64])
    p.add_argument('--blocks', type=int, default=3)
    p.add_argument('--flavor', choices=['dynamic-body', 'scalar'], default='dynamic-body')
    p.add_argument('--controls', choices=['call-edge', 'signature', 'cfg'], nargs='*', default=['call-edge', 'signature', 'cfg'])
    p.add_argument('--states', choices=['cold-cli', 'warm-cli', 'resident-cli'], nargs='+', default=['cold-cli', 'warm-cli', 'resident-cli'])
    p.add_argument('--cpus', type=int, nargs='+', required=True)
    p.add_argument('--frequency-policy', choices=['fixed', 'observed-uncontrolled'], required=True)
    p.add_argument('--fixed-khz', type=int)
    p.add_argument('--policy', choices=['default-o2', 'o2-after', 'incremental-o2', 'incremental-o2-after'], default='default-o2')
    a = p.parse_args()
    if a.blocks < 1 or (a.frequency_policy == 'fixed' and not a.fixed_khz):
        p.error('positive blocks and an explicit fixed kHz for fixed frequency are required')
    a.out = a.out.resolve()
    a.out.mkdir(parents=True, exist_ok=False)
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=a.repo, text=True).strip()
    protocol = {
        'format': 'zil-multi-file-edit-screen/1', 'status': 'prepared-not-run',
        'created_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'repo': str(a.repo.resolve()), 'read_only_checkout': source,
        'research_source': '5aa8057f9cc0bff651fc6a44147b4e6e2dd307de',
        'basis': 'tools/research/incremental-compilation/distribution_bench.py and PR78 resident JSON stdio',
        'existing_measurer_sha256': sha(a.repo / 'tools/research/incremental-compilation/distribution_bench.py'),
        'files': a.files, 'edits': a.edits, 'blocks': a.blocks, 'flavor': a.flavor,
        'scenario_weights': {key: {'cold-cli': .10, 'warm-cli': .45, 'resident-cli': .45}[key]
                             / sum({'cold-cli': .10, 'warm-cli': .45, 'resident-cli': .45}[item] for item in a.states)
                             for key in a.states},
        'weight_provenance': 'predeclared hypothetical request occurrence distribution; not measured user usage and not a CPU-clock-frequency claim',
        'edit_weights': {str(k): 1 / len(a.edits) for k in a.edits},
        'file_count_weights': {str(n): 1 / len(a.files) for n in a.files},
        'order': ['ABBA' if b % 2 == 0 else 'BAAB' for b in range(a.blocks)],
        'states': a.states,
        'cpus': a.cpus, 'frequency': {'policy': a.frequency_policy, 'fixed_khz': a.fixed_khz},
        'timeout_seconds': 180, 'native_timeout_seconds': 10,
        'policy': a.policy, 'optimization': 'O2', 'cgu': 8, 'jobs': 2,
        'resident_capacity': 256, 'resident_reset_after': 64,
        'controls': a.controls,
        'budget_control': {'status': 'separate-API-gate-required; CLI exposes no Limits override',
                           'existing_test': 'crates/zil-driver/src/session.rs::real_source_subquery_hits_replay_the_exact_current_budget_boundary',
                           'required': 'run existing N/N-1/fresh gate for exact candidate; no widening limits'},
        'seed_requests_per_warm_sequence': 1, 'measured_requests_per_arm_position': 1,
        'clean_oracle': 'one fresh baseline no-object-cache full-O2 compile per frozen snapshot/policy; mandatory ELF and sidecar hash identity for every A/B seed and edit',
        'artifact_comparison': 'ELF exact bytes; sidecar exact bytes after removing exactly one ; verified-fingerprint: diagnostic line; mismatch fails regardless of native output',
        'layout_recipe_observation': 'outside request timers: name/size/SHA256 sets under objects/partition-layout-v1 and v2 before and after every request; no forced same-key assertion when MachineCost/structure changes',
        'rust_samples': 'one CLI request per block and cell, 2 with two blocks; independent from 4 A/B samples per arm with two ABBA blocks',
        'runtime_gate': {'rust_minimum_relative': 0.9, 'maximum_relative_regression': 0.10,
                         'status': 'requires-existing-runtime-suite; fixture-output-only-is-not-a-runtime-performance-gate'},
        'fixed_toolchains_required': {'llvm': '23.1.4', 'rust': '1.99.1'},
        'timing': {'cli': 'Popen launch through wait4; native oracle and source writes outside clock',
                   'resident': 'write compile newline through complete JSON response; ready/startup separately recorded',
                   'cpu_cli': 'wait4 ru_utime+ru_stime, including reaped descendants per OS semantics',
                   'cpu_resident': '/proc/pid/stat process+reaped-child ticks delta; tick-resolution, not sampling',
                   'rss_cli': 'wait4 ru_maxrss: maximum individual process RSS, not aggregate tree peak',
                   'rss_resident': 'RSS after response; lifetime VmHWM includes seed and earlier work, not per-request peak'},
        'cache': 'cold: empty cache; warm/resident: one base seed then one final edited request; fresh cache per arm-position-k',
        'rust': 'optional existing direct rustc O2 semantic pair; overflow checks, panic abort, 8 CGU, per-sequence incremental; resident comparison not available',
        'exclusions': ['no parallel agent benchmark in this first screen', 'no source writes while compilation runs',
                       'no compilation after each changed file', 'no claiming low-tier response as O2 completion',
                       'no gate bypasses', 'no tuning after results without a new protocol/run identity'],
        'fixtures': {},
        'tools': {name: sha(Path(__file__).with_name(name)) for name in ['fixture.py', 'prepare.py', 'screen.py']},
    }
    for n in a.files:
        base = generate(a.out / f'n{n}' / 'base', n, 0, flavor=a.flavor)
        protocol['fixtures'][f'n{n}/base'] = base
        for k in a.edits:
            if k > n:
                p.error('k exceeds total files')
            item = generate(a.out / f'n{n}' / f'edit-{k}', n, k, flavor=a.flavor)
            for ext in ['zil', 'rs']:
                actual = [name for name, digest in item['source_hashes'].items()
                          if name.endswith('.' + ext) and digest != base['source_hashes'][name]]
                assert sorted(actual) == sorted(changed_names(n, k, ext)), (n, k, actual)
            protocol['fixtures'][f'n{n}/edit-{k}'] = item
        for kind in protocol['controls']:
            protocol['fixtures'][f'n{n}/control-{kind}'] = generate_control(a.out / f'n{n}' / f'control-{kind}', n, kind, flavor=a.flavor)
    (a.out / 'protocol.json').write_text(json.dumps(protocol, indent=2) + '\n')
    print(json.dumps({'status': 'prepared-not-run', 'protocol': str(a.out / 'protocol.json'),
                      'sha256': sha(a.out / 'protocol.json')}, indent=2))


if __name__ == '__main__':
    main()

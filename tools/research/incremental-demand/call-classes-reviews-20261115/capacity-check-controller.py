#!/usr/bin/env python3
"""expanded-large-capacity-v1: root-run, frozen-binary capacity observations.

Prepare (source hashes, helper hashes and pins; NO subprocesses):
  python3 -B /workspace/zil-recovery/expanded-large-capacity-01.py prepare \
    --out /absolute/capacity-plan
Root reviews/freezes the plan and this file, then SEPARATELY runs:
  python3 -B /workspace/zil-recovery/expanded-large-capacity-01.py measure \
    --freeze /absolute/capacity-plan/freeze.json --mode check --out /absolute/capacity-check
  python3 -B /workspace/zil-recovery/expanded-large-capacity-01.py measure \
    --freeze /absolute/capacity-plan/freeze.json --mode o2 --out /absolute/capacity-native

Exact original generated(100,100|250,1,label,False), short scalar dynamic I/O;
10105/25105 declarations, all 102 Zil files, no unused additions or trimming.
Source identity uses the existing source_metadata complete relative-path/hash
vector and byte counts. Every history snapshot is predeclared before admission.
Actual main9170 / candidate027 from call-classes-pins-05 / Rust1.99.1. O2,
CGU16/jobs1/default budgets. Candidate ONLY push-reuse, SDK18 native profile,
closed-executable, incremental-o2, merging off. Check is a separate run and uses
no native flags. Two blocks reverse the per-request order; history is cold,
unchanged, private1, error (private1 + absent_name), recovery (new private2),
restore and resident restart with the same physical caches.

Composition only: existing matched worker/process/archive/publication primitives,
current closed-import physical CGU/provenance/ABI checks, all256 bytes + EOF on
EVERY successful native request, eight functional workers after compile ends.
No memoized executions. Failed builds never execute earlier publications.
ResourceLimit remains INVALID; main/candidate failures continue the workflow.
Raw rows and physical disk boundaries survive; no ratio for an invalid cell.
A complete failed workflow is not accepted. Unexpected infrastructure failure
retains partial receipts and reports execution_complete=false.

LIMIT: this compact CAPACITY suite does not run the independent sustained10k
wrapper. Runtime quality/acceptance are UNKNOWN, thresholds remain the existing
1.10/.90/.95 and are not evaluated here. Changed ELF is recorded accurately and
admitted only by the existing finite scalar source/oracle/Verify/physical ABI
quality profile. There is no performance acceptance claim, universal ABI claim,
old full-ELF gate change, compiler build or new budget. Root alone executes.
Disk includes explicit original user cache/artifact roots, global inode dedup at
boundaries, not transient peaks. Content-addressed raw store is shared globally;
SDK/compiler evidence is archived once per run, never once per request.
"""
import argparse
import json
from pathlib import Path
import sys
import types

sys.dont_write_bytecode = True
DEFAULT_REPO = Path('/workspace/zil-push-main')
sys.path.insert(0, str(DEFAULT_REPO/'tools/research/incremental-demand'))
import closed_project_matrix as matrix

large, q = matrix.large, matrix.quality
rc, matched, pc = matrix.rc, matrix.matched, matrix.pc
require, compose = q.require, matrix.compose
SCHEMA = 'expanded-large-capacity-v1'
SIZES = {'10105': 100, '25105': 250}
PIN_SOURCES = {'baseline': '9170bb4210554392e36eb7528596612596ebef80',
               'candidate': '027695318436fcb1f8994d0dc9802a0b2349c362'}
PIN_BINARIES = {'baseline': '08fcf4bc3b31590068585b63f4f4bc18def552234b9c57295b9a4b0fb4f199f0',
                'candidate': '5b3a309a7860dca3a1d3f15e7c930ec7d3f3a3fa59286e6d91c67e0067c0743f'}
HISTORY = [('cold', 'cold'), ('unchanged', 'cold'), ('private-001', 'private'),
           ('error', 'error'), ('recovery', 'recovery'), ('restore', 'cold'),
           ('warm-restart', 'cold')]


def fixture(workloads, size, label):
    # Error retains private1's body except the genuine unresolved name.
    def delta(_label, i, j, _modules, _functions):
        return (2 if label == 'recovery' else int(label in ('private', 'error'))) * int(i == j == 0)
    helpers = SIZES[size]
    zil = compose(workloads.generated, delta=delta)(100, helpers, 1, label, False)
    rust = rc.rust_files(types.SimpleNamespace(delta=delta), label, 100, helpers, 1)
    equality = rc.validate_equivalence(zil, rust, 100, helpers, 1, label)
    require(len(zil) == 102, 'all original source files retained')
    return {'zil': zil, 'rust': rust}, equality, delta


class Oracle:
    def __init__(self, workloads, size, label, delta):
        self.workloads, self.size, self.label, self.delta = workloads, size, label, delta
        self.sustained = False
        self.cache = {}

    def inputs(self, _case):
        return q.INPUTS

    def oracle(self, _case, data, _label, *_dimensions):
        if data not in self.cache:
            self.cache[data] = compose(self.workloads.expected_generated, delta=self.delta)(
                data, 100, SIZES[self.size], 1, self.label, False)
        return self.cache[data]


def frozen_inputs(args, workloads):
    paths = matrix.implementation_paths(args.repo)
    paths += [args.repo/'tools/research/reactive-query-graph/native_workloads.py',
              args.repo/'examples/mainline/resource_closures/io.zil', Path(__file__).resolve()]
    paths = sorted(set(p.resolve() for p in paths))
    pins = json.loads(args.pins.read_text())
    large.controller.validate_pins(pins)
    for arm in PIN_SOURCES:
        require(pins[arm]['source'] == PIN_SOURCES[arm] and pins[arm]['binary_sha256'] == PIN_BINARIES[arm],
                'exact requested compiler/source pin')
    sources = {}
    for size in SIZES:
        sources[size] = {}
        for name, label in HISTORY:
            files, _, _ = fixture(workloads, size, label)
            sources[size][name] = {lang: q.source_metadata(text) for lang, text in files.items()}
    return {'schema': SCHEMA, 'protocol': __doc__, 'pins': pins,
            'implementation_sha256': {str(p): matched.file_hash(p) for p in paths},
            'sources': sources, 'dimensions': {s: [100, h, 1] for s, h in SIZES.items()},
            'history': [list(item) for item in HISTORY], 'blocks': 2, 'runtime_gates': q.RUNTIME_GATES,
            'runtime_acceptance': 'UNKNOWN: independent sustained profile not executed',
            'expected_rows_per_mode': 84}


def initialize(args, freeze):
    env = large.clean_environment()
    # External protocol binds exact imported bytes in freeze.json, independently
    # of the legacy tool-source commit/old ELF gate. Arm source/tree/build receipt
    # admission remains the original source_identity implementation.
    def verify_tools(_args, _source):
        for path, sha in freeze['implementation_sha256'].items():
            require(matched.file_hash(path) == sha, 'frozen implementation changed: '+path)

    def source_identity(repo, source, out, role):
        if role == 'tools':
            return {'source': source, 'scope': SCHEMA+' exact file hash vector; no repository HEAD claim',
                    'implementation_sha256': freeze['implementation_sha256']}
        return matched.source_identity(repo, source, out, role)

    matched_view = types.SimpleNamespace(**{**vars(matched), 'source_identity': source_identity})
    admission = types.SimpleNamespace(**{**vars(args), 'candidate_policy': 'push'})
    admit = compose(large.pin_inputs, verify_frozen_tools=verify_tools, matched=matched_view)
    receipt = admit(admission, freeze['pins'], env)
    for name in ('baseline', 'candidate', 'baseline_source', 'candidate_source', 'cargo', 'rustc', 'nm',
                 'native_link_profile', 'native_link_profile_sha256'):
        setattr(args, name, getattr(admission, name))
    return env, receipt


def zil_command(args, arm, source, work):
    if args.mode == 'o2':
        return matrix.zil_command(args, arm, source, work)
    return pc.zil_command(binary=getattr(args, arm), mode='check', source=source,
                          work=work, arm=arm, extras=['--push-reuse'])


def inspect_report(report, **kwargs):
    # Keep failed current reports intact; cold success assertions do not apply
    # to ResourceLimit. No changed status, manufactured Verify or zero timing.
    if report.get('status') != 'ResourceLimit':
        return pc.inspect_report(report, **kwargs)


def independent_quality(args, row, oracle, name):
    require(row.get('valid') is True, 'failed request is INVALID')
    if name == 'error':
        require(row['publication_before'] == row['publication_after'], 'error publication unchanged')
        require('native' not in row, 'error cannot execute old executable')
        return
    if row['arm'] != 'rust':
        require(q.current_verify(row['resident']['report']), 'current independent Verify required')
    if args.mode == 'o2':
        q.checked_oracles(row, oracle, 1)
        matched.recheck_archived(args.out, row['executable'])
        if row['arm'] != 'rust':
            inv = row['inventory']
            require(inv['object_count'] == len(inv['objects']) == inv['compiler_summary']['actual_units'] == 16
                    and inv['current_layout_matches'], 'actual physical CGU16 required')
            row['scalar_abi'] = q.executable_abi(args, row)
        else:
            require(row['inventory'].get('objects'), 'physical Rust objects required')


def workflow(args, workloads, native, env, environments, freeze):
    pc_view = types.SimpleNamespace(**{**vars(pc), 'inspect_report': inspect_report})
    zil_request = compose(matched.zil_request, pc=pc_view, native_outcomes=matrix.native_outcomes,
                          zil_inventory=q.inventory)
    rust_request = compose(matched.rust_request, native_outcomes=matrix.native_outcomes)
    start = compose(matrix.start_worker, zil_command=zil_command)
    rows, cells, validation_failures = [], [], []
    for size in SIZES:
        args.preset = size
        for block in range(2):
            base = args.out/'workflow'/size/f'block-{block:02d}'
            source, project, user = base/'sources/zil', base/'sources/rust', base/'user'
            project.mkdir(parents=True)
            (project/'Cargo.toml').write_text(rc.manifest_text(16))
            (project/'Cargo.lock').write_text(rc.lock_text())
            for arm in matched.ARMS[:2]:
                (user/arm).mkdir(parents=True)
            initial, _, _ = fixture(workloads, size, 'cold')
            rc.write_files(source, initial['zil'], '.zil')
            workers, previous, published = {}, {}, {}
            try:
                for arm in matched.ARMS[:2]:
                    workers[arm] = start(args, native, environments, arm, source, user, base/'resident'/arm/'0')
                for index, (name, label) in enumerate(HISTORY):
                    files, equality, delta = fixture(workloads, size, label)
                    identities = {lang: q.source_metadata(text) for lang, text in files.items()}
                    require(identities == freeze['sources'][size][name], 'predeclared complete source byte/hash binding')
                    meta = rc.write_files(source, files['zil'], '.zil')
                    leaf = base/'requests'/f'{index:02d}-{name}'
                    leaf.mkdir(parents=True)
                    rc.save(leaf/'source-equivalence.json', dict(equality, source=identities))
                    if name == 'warm-restart':
                        for arm in matched.ARMS[:2]:
                            matrix.close_worker(args, native, workers.pop(arm))
                            workers[arm] = start(args, native, environments, arm, source, user, base/'resident'/arm/'1')
                    oracle = Oracle(workloads, size, label, delta)
                    triplet = []
                    order = matched.ORDERS[index % 6]
                    if block:
                        order = list(reversed(order))
                    for arm in order:
                        directory = leaf/arm
                        before = large.disk_snapshot(large.disk_roots(user))
                        try:
                            if arm == 'rust':
                                row = rust_request(args, oracle, {name: files}, name, project, user/'rust-target', directory, env, previous)
                                previous = row['inventory']
                            else:
                                row = zil_request(args, oracle, {name: files}, name, source, meta, user, arm,
                                                  workers, published, directory, environments, cold=index == 0)
                            row['arm'] = arm
                            report = row.get('resident', {}).get('report', {})
                            row['resource_limit'] = report.get('status') == 'ResourceLimit'
                            compiler_succeeded = (report.get('status') == 'Success' if arm != 'rust' else
                                                  row['measurement']['returncode'] == 0 and not row['measurement']['timed_out'])
                            row['compiler_succeeded'] = compiler_succeeded
                            if not compiler_succeeded:
                                require(row['publication_before'] == row['publication_after'], 'failed build changed publication')
                                require('native' not in row, 'failed build executed old publication')
                            if row.get('valid'):
                                independent_quality(args, row, oracle, name)
                            else:
                                row['failure_kind'] = ('compiler-failure' if not compiler_succeeded else
                                                       'functional-validation-failure')
                        except Exception as error:
                            if 'row' not in locals() or row.get('arm') != arm:
                                row = {'arm': arm, 'valid': False}
                            row.update(valid=False, failure=type(error).__name__+': '+str(error))
                            validation_failures.append(dict(directory=str(directory.relative_to(args.out)),
                                                            failure=row['failure']))
                            # Original response/process files remain at directory,
                            # including errors raised inside inventory validation.
                        row.update(arm=arm, preset=size, block=block, index=index, request=name,
                                   source_identity_sha256=meta['identity_sha256'], order=order,
                                   disk_before=before, disk_after=large.disk_snapshot(large.disk_roots(user)),
                                   directory=str(directory.relative_to(args.out)))
                        triplet.append(row)
                        saved = matrix.slim(row, args.out)
                        rows.append(saved)
                        with (args.out/'rows.jsonl').open('a') as stream:
                            stream.write(json.dumps(saved)+'\n')
                        rc.save(directory/'capacity-request.json', saved)
                        del row
                    cell = {'valid': False, 'ratios': None, 'complete_ELF_equal': None}
                    if all(r['valid'] for r in triplet):
                        try:
                            cell = q.quality(triplet, args, oracle)
                            if args.mode == 'check':
                                cell.update(schema=SCHEMA+'/check', full_domain_oracle=None,
                                            functional_quality_valid=None, check_quality_valid=True,
                                            physical_native_checks=None,
                                            scope='Check only: complete source/diagnostics/current Verify; native oracle, ELF and physical ABI checks not applicable')
                            # Two-block comparisons are computed only after the
                            # entire relevant cell validates in both blocks.
                        except Exception as error:
                            cell.update(valid=False, failure=str(error))
                            validation_failures.append(dict(directory=str(leaf.relative_to(args.out)), failure=str(error)))
                    cell.update(preset=size, block=block, request=name,
                                role_status={r['arm']: ('valid' if r['valid'] else 'INVALID') for r in triplet})
                    cells.append(cell)
                    rc.save(leaf/'capacity-quality.json', cell)
                    print(json.dumps({k: cell[k] for k in ('preset', 'block', 'request', 'valid', 'role_status')}), flush=True)
            finally:
                shutdown = []
                for worker in workers.values():
                    try:
                        matrix.close_worker(args, native, worker)
                    except Exception as error:
                        shutdown.append(str(error))
                require(not shutdown, 'resident final identity: '+str(shutdown))
    return rows, cells, validation_failures


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('prepare', 'measure'))
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--repo', type=Path, default=DEFAULT_REPO)
    parser.add_argument('--pins', type=Path, default=DEFAULT_REPO/'out/runs/call-classes-pins-05.json')
    parser.add_argument('--freeze', type=Path)
    parser.add_argument('--mode', choices=('check', 'o2'))
    parser.add_argument('--cargo', default='cargo')
    parser.add_argument('--rustc', default='rustc')
    parser.add_argument('--nm', default='nm')
    parser.add_argument('--timeout', type=float, default=900)
    parser.add_argument('--native-timeout', type=float, default=120)
    parser.add_argument('--sample-ms', type=float, default=5)
    args = parser.parse_args(argv)
    require(args.repo.resolve() == DEFAULT_REPO, 'imports bound to declared shared repository')
    args.repo, args.out = args.repo.resolve(), args.out.absolute()
    require(not args.out.exists() and '..' not in args.out.parts, 'new output directory required')
    require(all(0 < x < float('inf') for x in (args.timeout, args.native_timeout, args.sample_ms)), 'finite positive timeouts')
    if args.action == 'measure':
        require(args.freeze and args.mode, 'explicit frozen plan and separate mode required')
    workloads = rc.load_workloads(args.repo)
    current = frozen_inputs(args, workloads)
    if args.action == 'prepare':
        args.out.mkdir(parents=True)
        rc.save(args.out/'freeze.json', current)
        print(json.dumps({'ready': True, 'freeze': str(args.out/'freeze.json'), 'subprocesses_started': 0}))
        return 0
    freeze = json.loads(args.freeze.read_text())
    require(current == freeze, 'controller/helpers/pins/all source recipes differ from predeclared freeze')
    args.candidate_policy, args.function_merging, args.complete_imports = 'closed', 'off', True
    args.candidate_extra_arg, args.native_repetitions, args.native_validation_jobs = ['--push-reuse'], 1, 8
    args.blocks, args.pins_data = 2, freeze['pins']
    args.out.mkdir(parents=True)
    manifest = dict(current, mode=args.mode, accepted=False, execution_complete=False,
                    argv=sys.argv, freeze_file_sha256=matched.file_hash(args.freeze))
    rc.save(args.out/'manifest.json', manifest)
    try:
        env, admission = initialize(args, freeze)
        manifest['admission'] = admission
        sys.modules['native_workloads'] = workloads
        native = matched.load_module('capacity_native_runtime', args.repo/'tools/research/reactive-query-graph/native_measure.py')
        environments = {arm: native.llvm_environment(freeze['pins'][arm]['llvm_library_path'], env)
                        for arm in matched.ARMS[:2]}
        rows, cells, validation_failures = workflow(args, workloads, native, env, environments, freeze)
        for arm in matched.ARMS[:2]:
            require(matched.file_hash(freeze['pins'][arm]['binary']) == PIN_BINARIES[arm], 'compiler changed during run')
        for path, sha in freeze['implementation_sha256'].items():
            require(matched.file_hash(path) == sha, 'helper/controller changed during run')
        if args.mode == 'o2':
            manifest['native_profile_archive'] = matched.archive_native_profile(args)
        comparisons = []
        for size in SIZES:
            for name, _ in HISTORY:
                good = [c for c in cells if c['preset'] == size and c['request'] == name]
                if len(good) == 2 and all(c['valid'] for c in good):
                    selected = [r for r in rows if r['preset'] == size and r['request'] == name]
                    comparisons.append(dict(preset=size, request=name, diagnostic_only=True,
                                            **q.paired_comparison(selected, 2, matched.ARMS)))
        complete = len(rows) == 84 and len(cells) == 28 and not validation_failures
        valid = complete and all(c['valid'] for c in cells)
        summary = {'schema': SCHEMA, 'execution_complete': complete, 'capacity_valid': valid,
                   'accepted': False, 'runtime_acceptance': 'UNKNOWN: not measured',
                   'rows': len(rows), 'cells': cells, 'comparisons': comparisons,
                   'infrastructure_or_validation_failures': validation_failures,
                   'disk_boundaries': 'rows.jsonl -> exact raw rows; global inode dedup over original user roots',
                   'limits': 'ResourceLimit and other failed arms INVALID, never comparison denominators'}
        manifest.update(execution_complete=complete, capacity_valid=valid)
        rc.save(args.out/'summary.json', summary)
        rc.save(args.out/'manifest.json', manifest)
        rc.save(args.out/'completion.json', dict(execution_complete=complete, capacity_valid=valid, accepted=False))
        return 0 if valid else 2
    except Exception as error:
        failure = {'error': type(error).__name__+': '+str(error), 'execution_complete': False, 'accepted': False}
        rc.save(args.out/'failure.json', failure)
        rc.save(args.out/'completion.json', failure)
        manifest['failure'] = failure
        rc.save(args.out/'manifest.json', manifest)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())

#!/usr/bin/env python3
"""Joint cache/time diagnostic, composed from closed_project_matrix unchanged.

Install as tools/research/incremental-demand/cache_lifecycle_matrix.py, freeze
that tool commit in pins.tool_source, then use plan/measure with --cache-state
retain|clear and the original parser's --repo/--out/--pins and timing options.
Default cells: 2500/5105, two blocks, O2/CGU16/compiler jobs1, closed+off with
complete incremental imports. History: cold, unchanged, warm-restart,
private-001, error, recovery, using the exact original history(8) requests.
Five separate sustained wrapper cases per size keep all existing runtime gates.
All original functional inputs (EOF and 256 bytes), current independent Verify,
physical units, source identities, pins and producer policies remain mandatory.
This measures retain versus full disposable-cache clearing only, not partial
history GC. Compiler restart cost is a diagnostic; a current negative is expected.
Cleanup and ZIL startup are separately recorded outside compile timers; Rust
request timing still includes Cargo startup. No user end-to-end total is invented.
"""
import argparse
import json
from pathlib import Path
import shutil
import stat
import sys
import time
import types

INSTALLED = Path('tools/research/incremental-demand/cache_lifecycle_matrix.py')
NAMES = ('cold', 'unchanged', 'warm-restart', 'private-001', 'error', 'recovery')
SCHEMAS = {'retain': 'closed-import-cache-retain-runtime-v1',
           'clear': 'closed-import-cache-clear-on-restart-runtime-v1'}


def main(argv=None):
    sys.dont_write_bytecode = True
    bootstrap = argparse.ArgumentParser(add_help=False, allow_abbrev=False)
    bootstrap.add_argument('--cache-state', required=True, choices=SCHEMAS)
    default = Path(__file__).resolve().parents[3] if Path(__file__).name == INSTALLED.name else Path('/workspace/zil-push-main')
    bootstrap.add_argument('--repo', type=Path, default=default)
    selected, forwarded = bootstrap.parse_known_args(argv)
    repo, state = selected.repo.resolve(), selected.cache_state
    sys.path.insert(0, str(repo/INSTALLED.parent))
    import closed_project_matrix as matrix
    compose, require, large, rc = matrix.compose, matrix.require, matrix.large, matrix.rc
    require(Path(__file__).resolve() == repo/INSTALLED, 'run the exact installed, frozen tool inside --repo')
    forwarded += ['--repo', str(repo)]
    present = {value.split('=', 1)[0] for value in forwarded}
    for flag, values in (('--sizes', ['2500', '5105']), ('--blocks', ['2']), ('--epochs', ['8']),
                         ('--candidate-policy', ['closed']), ('--function-merging', ['off']),
                         ('--complete-imports', [])):
        if flag not in present:
            forwarded += [flag, *values]
    lifecycle, closed, ownership = [], set(), {}
    policy = {'cache_state': state, 'schema': SCHEMAS[state], 'history': list(NAMES),
              'producer_policy_identity': 'closed+off+complete-imports',
              'clear_boundary': 'after old ZIL worker closes, before worker directory 1 starts; no Rust request active',
              'clear_paths': ['user/<closed-arm>/semantic', 'user/<closed-arm>/objects',
                              'user/rust-target at the candidate boundary'],
              'preserved': 'ZIL program/program.ll, source, pins, SDK, evidence, other ZIL arm; Rust target includes its exe',
              'partial_history_gc': False, 'restart_expectation': 'current compiler restart negative expected',
              'timing': 'compile scopes unchanged; cleanup/startup separate; no combined user end-to-end total'}

    def safe_path(path):
        require(path.is_absolute() and '..' not in path.parts, 'absolute nonescaping path required')
        for parent in (path, *path.parents):
            require(not parent.is_symlink(), 'symlink path refused: '+str(parent))
        require(path.resolve() == path, 'path resolution escaped expected root')

    class Parser(argparse.ArgumentParser):
        def parse_args(self, args=None, namespace=None):
            args = super().parse_args(args, namespace)
            require(args.epochs == 8 and args.blocks == 2, 'frozen history(8), two blocks required')
            require(set(args.sizes) <= {'2500', '5105'}, 'only original 2500/5105 cells')
            require(args.complete_imports and args.candidate_policy == 'closed'
                    and args.function_merging == 'off', 'closed+off+complete-imports required')
            args.cache_state = state
            safe_path(args.out.absolute())
            require(not args.out.exists(), 'own a new window root only')
            ownership['root'] = args.out.absolute()
            return args

    def implementation_paths(repo):
        root = ownership['root']
        safe_path(root)
        info = root.stat()
        require(root.is_dir(), 'new window root must be a directory')
        ownership.setdefault('inode', (info.st_dev, info.st_ino))
        require(ownership['inode'] == (info.st_dev, info.st_ino), 'window root replaced')
        return [*matrix.implementation_paths(repo), repo/INSTALLED]

    def history(epochs):
        require(epochs == 8, 'short history is selected from original history(8)')
        requests = {request['name']: request for request in matrix.history(8)}
        return [dict(requests[name]) for name in NAMES]

    def boundary(args, source, user, directory, arm):
        root = ownership['root']
        safe_path(root)
        info = root.stat()
        require((info.st_dev, info.st_ino) == ownership['inode'], 'window root ownership changed')
        parts = directory.relative_to(root).parts
        require(len(parts) == 6 and parts[0] in ('workflow', 'runtime-profile')
                and parts[1] == args.preset and parts[2] in
                ({'block-00'} if parts[0] == 'runtime-profile' else {'block-00', 'block-01'})
                and parts[3:] == ('resident', arm, directory.name), 'unexpected worker path')
        base = root.joinpath(*parts[:3])
        require(user == base/'user' and source == base/'sources'/'zil', 'unexpected disposable user/source root')
        require(arm in matrix.matched.ARMS[:2] and directory.name in ('0', '1'), 'known worker only')
        safe_path(user); safe_path(directory)
        return base

    def checked_tree(path):
        safe_path(path)
        require(path.is_relative_to(ownership['root']), 'cache escaped owned window')
        if not path.exists():
            return
        require(path.is_dir(), 'known cache root must be a directory')
        pending = [path]
        while pending:
            item = pending.pop()
            info = item.lstat()
            require(info.st_dev == ownership['inode'][0] and
                    (stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode)),
                    'symlink, mount or special file in disposable cache: '+str(item))
            if stat.S_ISDIR(info.st_mode):
                pending.extend(item.iterdir())

    def record(args, path, event):
        event.update(schema=SCHEMAS[state], cache_state=state, outside_compile_timer=True)
        lifecycle.append(event)
        rc.save(path, event)

    def close_worker(args, native, worker):
        matrix.close_worker(args, native, worker)
        require(worker.process.poll() is not None, 'old worker must be closed before cleanup')
        closed.add(worker.directory)

    def start_worker(args, native, environments, arm, source, user, directory):
        base = boundary(args, source, user, directory, arm)
        restart = directory.name == '1'
        if restart:
            require(directory.with_name('0') in closed, 'restart predecessor not closed')
            started = time.perf_counter_ns()
            paths = [user/arm/'semantic', user/arm/'objects']
            if arm == 'candidate':
                paths.append(user/'rust-target')
            for path in paths:
                checked_tree(path)
            before = large.disk_snapshot(large.disk_roots(user))
            deleting = time.perf_counter_ns()
            completed = False
            try:
                if state == 'clear':
                    require(shutil.rmtree.avoids_symlink_attacks, 'fd-safe cache deletion required')
                    for path in paths:
                        if path.exists():
                            shutil.rmtree(path)
                    require(not any(path.exists() or path.is_symlink() for path in paths), 'cache clear incomplete')
                completed = True
            finally:
                delete_ns = time.perf_counter_ns()-deleting
                after = large.disk_snapshot(large.disk_roots(user))
                record(args, base/'cache-lifecycle'/f'{arm}-restart.json', {
                    'kind': 'restart-cache-operation', 'directory': str(directory.relative_to(args.out)),
                    'arm': arm, 'paths': [str(path.relative_to(args.out)) for path in paths],
                    'action': 'full-clear' if state == 'clear' else 'retain', 'completed': completed,
                    'before': before, 'after': after, 'operation_ns': time.perf_counter_ns()-started,
                    'delete_ns': delete_ns, 'scope': 'validation, snapshots, deletion; receipt writing excluded'})
        started = time.perf_counter_ns()
        try:
            worker = matrix.start_worker(args, native, environments, arm, source, user, directory)
        except BaseException:
            record(args, base/'cache-lifecycle'/f'{arm}-startup-{directory.name}.json', {
                'kind': 'worker-startup-failed', 'arm': arm, 'directory': str(directory.relative_to(args.out)),
                'start_hook_ns': time.perf_counter_ns()-started})
            raise
        hook_ns = time.perf_counter_ns()-started
        try:
            startup = json.loads((directory/'startup.json').read_text())
            record(args, base/'cache-lifecycle'/f'{arm}-startup-{directory.name}.json', {
                'kind': 'worker-startup', 'arm': arm, 'directory': str(directory.relative_to(args.out)),
                'reported_startup_wall_ms': startup['wall_ms'], 'start_hook_ns': hook_ns,
                'scope': 'original startup.json wall; hook additionally includes runtime binding/archive checks'})
        except BaseException:
            worker.close()
            raise
        return worker

    def disk_summary(rows):
        results = []
        for size, block in sorted({(row['preset'], row['block']) for row in rows}):
            selected = [r for r in rows if (r['preset'], r['block']) == (size, block)]
            bounds = {r['request']: r['disk_after'] for r in selected}
            require(set(bounds) == set(NAMES), 'all six disk boundaries required')
            prefix = f'workflow/{size}/block-{block:02d}/'
            operations = [r for r in lifecycle if r['kind'] == 'restart-cache-operation'
                          and r['directory'].startswith(prefix)]
            require(len(operations) == 2 and {r['arm'] for r in operations} == {'baseline', 'candidate'}
                    and all(r['completed'] for r in operations), 'both complete restart-cache receipts required')
            snapshots = [r[edge] for r in selected for edge in ('disk_before', 'disk_after')]
            snapshots += [r[edge] for r in operations for edge in ('before', 'after')]
            categories = sorted({c for snapshot in snapshots for c in snapshot['categories']})
            def count(snapshot, category, metric):
                return snapshot['categories'].get(category, {}).get(metric, 0)
            results.append({'preset': size, 'block': block, 'request_boundaries': bounds,
                'categories': {c: {'observed_boundary_peak': {
                    m: max(count(snapshot, c, m) for snapshot in snapshots) for m in large.METRICS},
                    'restart_boundary_delta': {m: count(bounds['warm-restart'], c, m)-count(bounds['unchanged'], c, m)
                                               for m in large.METRICS}} for c in categories},
                'restart_operations': operations, 'transient_peak': None})
        return results

    q = matrix.quality
    schema = compose(q.schema, SCHEMA=SCHEMAS[state], IMPORT_SCHEMA=SCHEMAS[state])
    quality = types.SimpleNamespace(**{**vars(q), 'schema': schema,
        'quality': compose(q.quality, schema=schema),
        'runtime_decision': compose(q.runtime_decision, schema=schema)})
    initialize_original = compose(matrix.initialize, implementation_paths=implementation_paths)
    def initialize(args, manifest):
        manifest['cache_lifecycle_policy'] = policy
        return initialize_original(args, manifest)
    generate_original = compose(matrix.generate, history=history, quality=quality)
    def generate(args, workloads, manifest):
        manifest['cache_lifecycle_policy'] = policy
        if 'run_argv' in manifest:
            manifest['run_argv'] += ['--cache-state', state]
        return generate_original(args, workloads, manifest)
    report_original = compose(matrix.report, history=history, disk_summary=disk_summary, quality=quality)
    def report(args, manifest, rows, pairs, runtime):
        result = report_original(args, manifest, rows, pairs, runtime)
        summary = json.loads((args.out/'summary.json').read_text())
        summary.update(cache_lifecycle_policy=policy, cleanup_and_startup=lifecycle)
        summary['compile_request_totals'] = summary.pop('workflow_waiting_totals')
        rc.save(args.out/'summary.json', summary)
        return result
    run_window = compose(matrix.run_window, history=history, quality=quality,
                         start_worker=start_worker, close_worker=close_worker)
    parser_view = types.SimpleNamespace(**{**vars(argparse), 'ArgumentParser': Parser})
    return compose(matrix.main, argparse=parser_view, quality=quality, history=history,
                   implementation_paths=implementation_paths, initialize=initialize,
                   generate=generate, report=report, run_window=run_window,
                   __file__=str(repo/INSTALLED), __doc__=__doc__)(forwarded)


if __name__ == '__main__':
    raise SystemExit(main())

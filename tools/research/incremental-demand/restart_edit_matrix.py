#!/usr/bin/env python3
"""Direct first edit after compiler restart, with retained user caches.

History is cold, unchanged, close/edit/restart/private-001, error, recovery.
There is no unchanged compilation between restarting and the first edit. The
original 5105-function source recipes, current independent Verify, O2/CGU16/
compiler jobs1, EOF+256-byte oracles and all sustained runtime gates remain.
Only actual main, candidate and Rust are compared, in two balanced blocks.
Startup is recorded separately from compile requests. SDK identity auditing
does not copy the SDK; full archives require an explicit separate request.
"""
import argparse
import json
from pathlib import Path
import sys
import types

INSTALLED = Path('tools/research/incremental-demand/restart_edit_matrix.py')
NAMES = ('cold', 'unchanged', 'private-001', 'error', 'recovery')


def summarize_disks(rows, metrics):
    result = []
    for size in sorted({row['preset'] for row in rows}):
        for block in sorted({row['block'] for row in rows if row['preset'] == size}):
            selected = [row for row in rows if row['preset'] == size and row['block'] == block]
            boundaries = {row['request']: row['disk_after'] for row in selected}
            if not set(NAMES) <= set(boundaries):
                raise ValueError('incomplete direct restart edit disk history')
            categories = sorted({name for row in selected for name in row['disk_after']['categories']})
            def count(snapshot, category, metric):
                return snapshot['categories'].get(category, {}).get(metric, 0)
            first, last = selected[0]['disk_before'], selected[-1]['disk_after']
            result.append({'preset': size, 'block': block, 'categories': {category: {
                'observed_boundary_peak': {metric: max(count(row[edge], category, metric)
                    for row in selected for edge in ('disk_before', 'disk_after')) for metric in metrics},
                'growth_from_empty': {metric: count(last, category, metric)-count(first, category, metric)
                    for metric in metrics},
                'growth_from_cold': {metric: count(last, category, metric)-count(boundaries['cold'], category, metric)
                    for metric in metrics},
                'direct_restart_edit_growth': {metric: count(boundaries['private-001'], category, metric)
                    -count(boundaries['unchanged'], category, metric) for metric in metrics}}
                for category in categories}, 'transient_peak': None})
    return result


def main(argv=None):
    sys.dont_write_bytecode = True
    bootstrap = argparse.ArgumentParser(add_help=False, allow_abbrev=False)
    bootstrap.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[3])
    selected, forwarded = bootstrap.parse_known_args(argv)
    repo = selected.repo.resolve()
    sys.path.insert(0, str(repo / INSTALLED.parent))
    import closed_project_matrix as matrix
    compose, require, rc = matrix.compose, matrix.require, matrix.rc
    require(Path(__file__).resolve() == repo / INSTALLED, 'run the installed frozen tool')
    forwarded += ['--repo', str(repo)]
    present = {value.split('=', 1)[0] for value in forwarded}
    for flag, values in (('--sizes', ['5105']), ('--blocks', ['2']), ('--epochs', ['8']),
                         ('--candidate-policy', ['closed']), ('--function-merging', ['off']),
                         ('--complete-imports', [])):
        if flag not in present:
            forwarded += [flag, *values]
    policy = {'schema': 'closed-import-direct-restart-edit-runtime-v1',
              'history': list(NAMES), 'first_request_after_restart': 'private-001',
              'unchanged_prewarm_after_restart': False, 'cache_state': 'retain',
              'startup': 'separate; Rust Cargo startup remains inside each Rust request'}
    closed, lifecycle = set(), []

    class Parser(argparse.ArgumentParser):
        def parse_args(self, args=None, namespace=None):
            args = super().parse_args(args, namespace)
            require(args.sizes == ['5105'] and args.blocks == 2 and args.epochs == 8,
                    'frozen 5105 history(8) source recipes, two blocks required')
            require(args.complete_imports and args.candidate_policy == 'closed'
                    and args.function_merging == 'off', 'explicit closed incremental O2 import policy required')
            if args.bounded_class_imports:
                policy['schema'] = 'closed-bounded-class-direct-restart-edit-runtime-v1'
                policy['class_import_policy'] = 'bounded-classes-512-v2'
            else:
                policy['class_import_policy'] = 'complete'
            return args

    def history(epochs):
        require(epochs == 8, 'select exact original history(8) inputs')
        requests = {request['name']: request for request in matrix.history(8)}
        return [dict(requests[name]) for name in NAMES]

    def implementation_paths(root):
        return [*matrix.implementation_paths(root), root / INSTALLED]

    def close_worker(args, native, worker):
        matrix.close_worker(args, native, worker)
        require(worker.process.poll() is not None, 'old worker must end before restart')
        closed.add(worker.directory)

    def start_worker(args, native, environments, arm, source, user, directory):
        if directory.name == '1':
            require(directory.with_name('0') in closed, 'no overlapping old worker')
            base = directory.parents[2]
            binding = base / 'requests/002-private-001/source-equivalence.json'
            require(binding.is_file(), 'write the actual first edit before restart')
            current = json.loads(binding.read_text())
            require(current['request']['name'] == 'private-001', 'first restart input is edited')
            event = {'arm': arm, 'directory': str(directory.relative_to(args.out)),
                     'first_request': 'private-001', 'source_identity': current['source']['zil'],
                     'caches_cleared': False}
            lifecycle.append(event)
            rc.save(base / 'restart-edit' / (arm + '.json'), event)
        return matrix.start_worker(args, native, environments, arm, source, user, directory)

    initialize_original = matrix.initialize
    def initialize(args, manifest):
        manifest['direct_restart_edit_policy'] = policy
        return initialize_original(args, manifest)

    generate_original = compose(matrix.generate, history=history)
    def generate(args, workloads, manifest):
        manifest['direct_restart_edit_policy'] = policy
        return generate_original(args, workloads, manifest)

    report_original = compose(matrix.report, history=history,
        disk_summary=lambda rows: summarize_disks(rows, matrix.large.METRICS))
    def report(args, manifest, rows, pairs, runtime):
        require(len(lifecycle) == (args.blocks + 1) * 2,
                'both arms restart in every workflow block and runtime profile')
        result = report_original(args, manifest, rows, pairs, runtime)
        summary = json.loads((args.out / 'summary.json').read_text())
        summary.update(direct_restart_edit_policy=policy, restart_events=lifecycle)
        rc.save(args.out / 'summary.json', summary)
        return result

    run_window = compose(matrix.run_window, history=history,
                         restarts_worker=lambda request: request['name'] == 'private-001',
                         start_worker=start_worker, close_worker=close_worker)
    parser_view = types.SimpleNamespace(**{**vars(argparse), 'ArgumentParser': Parser})
    return compose(matrix.main, argparse=parser_view, history=history,
                   implementation_paths=implementation_paths, initialize=initialize,
                   generate=generate, report=report, run_window=run_window,
                   __file__=str(repo / INSTALLED), __doc__=__doc__)(forwarded)


if __name__ == '__main__':
    raise SystemExit(main())

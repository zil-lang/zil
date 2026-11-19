#!/usr/bin/env python3
"""Replay the frozen Rust project comparison and its semantic controls."""

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile


ROOT = Path(__file__).resolve().parents[2]
INPUTS = ROOT / 'validation/inputs/fixed-projects'
WORK_PROFILES = json.loads((ROOT / 'validation/language-alpha-resource-profile.json').read_bytes())['profiles']


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')


def git(repo, *args):
    return subprocess.check_output(['git', '-C', repo, *args], timeout=90)


def archive_project(repo, project, commit, destination):
    path = f"examples/{project['label']}"
    revision = f'{commit}:{path}'
    tree = git(repo, 'rev-parse', revision).decode().strip()
    if tree != project['tree']:
        raise ValueError(f'{revision} has tree {tree}, expected {project["tree"]}')
    destination.mkdir(parents=True)
    archive = git(repo, 'archive', revision)
    with tarfile.open(fileobj=io.BytesIO(archive)) as files:
        files.extractall(destination, filter='data')


def require_digest(path, expected):
    if digest(path.read_bytes()) != expected:
        raise ValueError(f'{path} does not match its recorded SHA-256 {expected}')


def fixtures(manifest, stdin_directory):
    inputs = {}
    for identity, recipe in manifest['inputs'].items():
        supplied = stdin_directory / identity if stdin_directory is not None else None
        if supplied is not None and supplied.is_file():
            data = supplied.read_bytes()
        elif recipe is None:
            continue
        elif 'byte_range' in recipe:
            data = bytes(range(recipe['byte_range']))
        else:
            data = bytes.fromhex(recipe['hex']) * recipe.get('repeat', 1)
        if digest(data) != identity:
            raise ValueError(f'Stdin bytes do not match SHA-256 {identity}')
        inputs[identity] = data
    return inputs


def execute(args, directory, stem, data=None):
    argv = [str(arg) for arg in args]
    try:
        result = subprocess.run(argv, input=data, capture_output=True, timeout=90)
        stdout, stderr, exit_code = result.stdout, result.stderr, result.returncode
        error = None
    except subprocess.TimeoutExpired as failure:
        stdout, stderr, exit_code = failure.stdout or b'', failure.stderr or b'', None
        error = 'timeout'
    except OSError as failure:
        stdout, stderr, exit_code = b'', b'', None
        error = str(failure)
    (directory / f'{stem}.stdout').write_bytes(stdout)
    (directory / f'{stem}.stderr').write_bytes(stderr)
    return {'argv': argv, 'exit_code': exit_code, 'error': error,
            'stdout_sha256': digest(stdout), 'stderr_sha256': digest(stderr)}


def zil_command(args, directory, stem, *command):
    invocation = execute([args.cli, *command, '--work-profile', args.work_profile, '--json'],
                         directory, stem)
    try:
        report = json.loads((directory / f'{stem}.stdout').read_bytes())
    except (ValueError, UnicodeError):
        report = {}
    return invocation, report


def verified(invocation, report):
    return (invocation['exit_code'] == 0 and report.get('status') == 'Success'
            and report.get('verification_runs') == 1)


def matches_work_profile(report, work_profile):
    policy = WORK_PROFILES[work_profile]
    return (report.get('work_profile') == work_profile
            and report.get('work_profile_revision') == policy['revision']
            and report.get('effective_limits') == policy['effective_limits'])


def matches_native_build(report, backend, opt, work_profile):
    codegen = report.get('native_codegen') or {}
    if (report.get('phase') != 'link' or not matches_work_profile(report, work_profile)
            or codegen.get('backend') != backend or 'fallback' not in codegen
            or codegen['fallback'] is not None):
        return False
    if backend == 'llvm-api':
        objects = codegen.get('object_reports', [])
        return (len(objects) == codegen.get('actual_units') and len(objects) > 0
                and all(obj.get('backend') == backend and obj.get('pipeline') == f'default<{opt}>'
                        for obj in objects))
    if backend == 'clang':
        # A linker invocation can carry -O2 even when no IR was compiled at O2.
        ir_commands = [cmd for cmd in report.get('commands', [])
                       if ('-x', 'ir') in zip(cmd, cmd[1:])]
        return bool(ir_commands) and all([flag for flag in cmd if flag.startswith('-O')] == [f'-{opt}']
                                         for cmd in ir_commands)
    return False


def check_source(args, directory, source, expected=None, stem='check'):
    invocation, report = zil_command(args, directory, stem, 'check', source)
    passed = verified(invocation, report) and report.get('phase') == 'verify'
    diagnostics = report.get('diagnostics', [])
    if expected is not None:
        passed = False
        if (invocation['exit_code'] not in (None, 0) and report.get('status') == 'SafeReject'
                and report.get('phase') == expected['phase'] and report.get('verification_runs') == 0
                and diagnostics):
            first = diagnostics[0]
            source_file = report.get('source_files', {}).get(str((first.get('span') or {}).get('file')))
            passed = (first.get('code') == expected['code'] and first.get('message') == expected['message']
                      and source_file is not None and Path(source_file).name == expected['file'])
    return {'passed': passed and matches_work_profile(report, args.work_profile),
            'work_profile': report.get('work_profile'), 'work_profile_revision': report.get('work_profile_revision'),
            'effective_limits': report.get('effective_limits'), 'invocation': invocation, 'status': report.get('status'),
            'phase': report.get('phase'), 'verification_runs': report.get('verification_runs'),
            'verification_usage': report.get('verification_usage'), 'diagnostics': diagnostics}


def run_native(args, directory, source, reference, entry, cases, source_passed, *,
               rust_args=(), require_empty_stderr=False):
    builds = []
    for backend, opt in args.manifest['configurations']:
        build = {'entry': entry, 'backend': backend, 'opt': opt, 'status': 'source-did-not-verify', 'runs': []}
        builds.append(build)
        build_dir = directory / f'{backend}-{opt}'
        zil, rust = build_dir / 'zil-program', build_dir / 'rust-program'
        can_run = False
        if source_passed:
            build_dir.mkdir(parents=True)
            build_args = ['build', source, '--backend', backend, '--opt', opt, '-o', zil]
            if entry != 'main':
                build_args.extend(['--entry', entry])
            invocation, report = zil_command(args, build_dir, 'zil-build', *build_args)
            build.update(zil=invocation, phase=report.get('phase'),
                         verification_runs=report.get('verification_runs'),
                         native_configuration_matched=matches_native_build(report, backend, opt, args.work_profile),
                         diagnostics=report.get('diagnostics', []), status='build-failed')
            if reference is not None:
                build['rust'] = execute(['rustc', '--edition=2024', f'-Copt-level={opt[1:]}',
                                         reference, '-o', rust], build_dir, 'rust-build')
            can_run = (verified(invocation, report) and build['native_configuration_matched'] and zil.is_file()
                       and (reference is None or (build['rust']['exit_code'] == 0 and rust.is_file())))
            if can_run:
                build['status'] = 'pass'
        for index, (identity, data, expected_exit, expected_stdout) in enumerate(cases):
            row = {'stdin_sha256': identity, 'expected_exit': expected_exit,
                   'expected_stdout_sha256': expected_stdout, 'status': 'build-unavailable'}
            build['runs'].append(row)
            if data is None:
                row['status'] = 'input-bytes-unavailable'
                continue
            if not can_run:
                continue
            row['zil'] = execute([zil], build_dir, f'{index}-zil', data)
            if reference is not None:
                row['rust'] = execute([rust, *rust_args], build_dir, f'{index}-rust', data)
                oracle = row['rust']
                same = row['zil']['exit_code'] == oracle['exit_code'] and all(
                    (build_dir / f'{index}-zil.{stream}').read_bytes() ==
                    (build_dir / f'{index}-rust.{stream}').read_bytes()
                    for stream in ('stdout', 'stderr'))
            else:
                oracle = row['zil']
                same = oracle['stderr_sha256'] == digest(b'')
            expected = (oracle['exit_code'], oracle['stdout_sha256']) == (expected_exit, expected_stdout)
            if require_empty_stderr:
                expected = expected and oracle['stderr_sha256'] == digest(b'')
            row['status'] = ('pass' if same and expected and row['zil']['error'] is None
                             and oracle['error'] is None else 'mismatch')
    return builds


def inline_cases(plan):
    for case in plan['cases']:
        data = bytes.fromhex(case['stdin_hex'])
        yield digest(data), data, case['expected_exit'], case['expected_stdout']


def check_control(args, project, directory):
    control = project['control']
    source_dir = directory / 'source'
    archive_project(args.repo, dict(control, label=project['label']), control['source_commit'], source_dir)
    for filename, replacement in control.get('replacements', {}).items():
        path = source_dir / filename
        data = path.read_bytes()
        for old, new in replacement['edits']:
            if data.count(old.encode()) != 1:
                raise ValueError(f'{path} lacks a unique occurrence of {old!r}')
            data = data.replace(old.encode(), new.encode(), 1)
        path.write_bytes(data)
        require_digest(path, replacement['sha256'])
    source = source_dir / 'main.zil'
    reference = source_dir / project['reference']['path']
    require_digest(reference, project['reference']['sha256'])
    report = check_source(args, directory, source)
    native = run_native(args, directory, source, reference, control['entry'], list(inline_cases(control)),
                        report['passed'], require_empty_stderr=True)
    result = {'obligation': control['obligation'], 'source_sha256': digest(source.read_bytes()),
              'check': report, 'native': native}
    if control.get('cleanup_control'):
        fixture = INPUTS / 'run-if-cleanup.zil'
        witness = source_dir / fixture.name
        witness.write_bytes(fixture.read_bytes())
        witness_dir = directory / 'cleanup'
        witness_dir.mkdir()
        check = check_source(args, witness_dir, witness)
        witness_cases = [(digest(b''), b'', 0, digest(b'SCD'))]
        result['cleanup'] = {'check': check, 'source_sha256': digest(witness.read_bytes()),
                             'native': run_native(args, witness_dir, witness, None, 'main', witness_cases,
                                                  check['passed'])}
    return result


def check_project(args, project, inputs):
    directory = args.out / 'projects' / project['label']
    source_dir = directory / 'source'
    archive_project(args.repo, project, args.manifest['frozen_source_commit'], source_dir)
    source = source_dir / 'main.zil'
    require_digest(source, project['source_sha256'])
    reference = source_dir / project['reference']['path']
    require_digest(reference, project['reference']['sha256'])
    candidate_path = f"{args.candidate}:examples/{project['label']}"
    try:
        candidate_tree = git(args.repo, 'rev-parse', '--verify', '--quiet', candidate_path).decode().strip()
    except subprocess.CalledProcessError:
        candidate_tree = None
    matches_original = candidate_tree == project['tree']
    matches_normalized = candidate_tree is not None and candidate_tree == project.get('normalized_example_tree')
    expected = project.get('expected_rejection')
    checked = check_source(args, directory, source, expected)
    row = {'label': project['label'], 'tree': project['tree'], 'source_sha256': project['source_sha256'],
           'candidate_tree': candidate_tree, 'candidate_matches_original_tree': matches_original,
           'candidate_matches_normalized_example_tree': matches_normalized,
           'reference': project['reference'], 'expected_rejection': expected, 'check': checked,
           'native': [], 'additional_native': []}
    if 'native' in project:
        plan = project['native']
        cases = [(r['stdin_sha256'], inputs.get(r['stdin_sha256']), r['expected_exit'], r['expected_stdout'])
                 for r in plan['runs']]
        row['native'] = run_native(args, directory, source, reference, plan['entry'], cases, checked['passed'])
    for plan in project.get('additional_native', []):
        entry = plan['entry']
        builds = run_native(args, directory / 'additional-native' / entry, source, reference, entry,
                            list(inline_cases(plan)), checked['passed'], rust_args=plan.get('rust_args', ()),
                            require_empty_stderr=True)
        row['additional_native'].extend(builds)
    if 'control' in project:
        row['control'] = check_control(args, project, directory / 'control')
    return row


def native_totals(builds, archive_if_unavailable=None):
    runs = [row for build in builds for row in build['runs']]
    unavailable = Counter(row['stdin_sha256'] for row in runs if row['status'] == 'input-bytes-unavailable')
    archived = {}
    if archive_if_unavailable:
        selected = Counter(row['stdin_sha256'] for row in runs)
        for identity, expected_rows in archive_if_unavailable.items():
            if expected_rows <= 0 or selected[identity] != expected_rows:
                raise ValueError(f'Historical stdin {identity} must identify exactly {expected_rows} frozen rows')
            if unavailable[identity]:
                if unavailable[identity] != expected_rows:
                    raise ValueError(f'Inconsistent availability for historical stdin {identity}')
                archived[identity] = unavailable[identity]
    return {'builds_selected': len(builds), 'builds_passed': sum(b['status'] == 'pass' for b in builds),
            'comparisons_selected': len(runs), 'comparisons_passed': sum(r['status'] == 'pass' for r in runs),
            'comparisons_required': len(runs) - sum(archived.values()),
            'archived_unavailable_by_input': archived, 'inputs_unavailable': sorted(unavailable)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=ROOT)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--build-receipt', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--stdin-directory', type=Path, help='Optional directory of original inputs named by SHA-256')
    parser.add_argument('--work-profile', choices=('standard', 'alpha'), default='alpha')
    parser.add_argument('--jobs', type=int, default=2)
    args = parser.parse_args()
    args.cli, args.out = args.cli.resolve(), args.out.resolve()
    if args.stdin_directory is not None and not args.stdin_directory.is_dir():
        parser.error('--stdin-directory must name an existing directory')
    receipt = json.loads(args.build_receipt.read_bytes())
    require_digest(args.cli, receipt['cli_sha256'])
    if (receipt['exit_code'] != 0 or
            git(args.repo, 'rev-parse', '--verify', f"{receipt['source_commit']}^{{commit}}").decode().strip()
            != receipt['source_commit']):
        parser.error('The build receipt must identify a successful build and a full source commit')
    manifest_bytes = (INPUTS / 'manifest.json').read_bytes()
    args.manifest = json.loads(manifest_bytes)
    args.candidate = receipt['source_commit']
    try:
        inputs = fixtures(args.manifest, args.stdin_directory)
    except ValueError as error:
        parser.error(str(error))
    args.out.mkdir(parents=True)
    input_output = args.out / 'stdin'
    input_output.mkdir()
    for identity, data in inputs.items():
        (input_output / identity).write_bytes(data)
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        rows = list(pool.map(lambda project: check_project(args, project, inputs), args.manifest['projects']))
    native = native_totals([build for row in rows for build in row['native']],
                           args.manifest['historical_contract']['archive_if_stdin_unavailable'])
    additional_native = native_totals([build for row in rows for build in row['additional_native']])
    controls = [row['control'] for row in rows if 'control' in row]
    witnesses = [control['cleanup'] for control in controls if 'cleanup' in control]
    control_native = native_totals([build for control in controls for build in control['native']])
    cleanup_native = native_totals([build for witness in witnesses for build in witness['native']])
    totals = args.manifest['fixed_totals']
    if (len(rows), native['builds_selected'], native['comparisons_selected']) != (
            totals['projects'], totals['native_builds'], totals['native_comparisons']):
        raise ValueError('The replay does not cover the frozen project and native denominators')
    additional_totals = args.manifest['additional_native_totals']
    if (additional_native['builds_selected'], additional_native['comparisons_selected']) != (
            additional_totals['native_builds'], additional_totals['native_comparisons']):
        raise ValueError('The replay does not cover the additional native denominators')
    candidate_trees_matched = sum(row['candidate_matches_original_tree'] or
                                  row['candidate_matches_normalized_example_tree'] for row in rows)
    summary = {'original_projects': len(rows),
               'source_resource_exhausted': [row['label'] for row in rows if row['check']['status'] == 'ResourceLimit'],
               'candidate_project_trees_matched': candidate_trees_matched,
               'candidate_original_trees_matched': sum(row['candidate_matches_original_tree'] for row in rows),
               'originals_verified': sum(row['check']['passed'] for row in rows if row['expected_rejection'] is None),
               'expected_rejections_selected': sum(row['expected_rejection'] is not None for row in rows),
               'expected_rejections': sum(row['check']['passed'] for row in rows if row['expected_rejection'] is not None),
               'original_native': native,
               'additional_native': additional_native,
               'additional_native_by_project': {
                   row['label']: native_totals(row['additional_native']) for row in rows if row['additional_native']
               },
               'controls_selected': len(controls), 'controls_verified': sum(c['check']['passed'] for c in controls),
               'control_native': control_native, 'cleanup_control_native': cleanup_native}
    passed = (candidate_trees_matched == len(rows)
              and all(row['check']['passed'] for row in rows)
              and all(c['check']['passed'] for c in controls + witnesses)
              and all(t['builds_passed'] == t['builds_selected'] and
                      t['comparisons_passed'] == t['comparisons_required']
                      for t in (native, additional_native, control_native, cleanup_native)))
    write_json(args.out / 'report.json', {'candidate': receipt['source_commit'], 'cli_sha256': receipt['cli_sha256'],
                                        'manifest_sha256': digest(manifest_bytes), 'work_profile': args.work_profile,
                                        'passed': passed, 'summary': summary, 'projects': rows})
    print(json.dumps(summary, indent=2))
    return 0 if passed else 1


if __name__ == '__main__':
    raise SystemExit(main())

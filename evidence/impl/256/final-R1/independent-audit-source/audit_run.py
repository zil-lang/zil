#!/usr/bin/env python3
"""Read-only GitHub CI evidence audit. All outputs stay in --output, never restore native trees.

Usage: python3 audit_run.py RUN_ID EXPECTED_HEAD --repo /workspace/zil-core
Network calls use the existing gh credentials. Run under the approved network executor.
Mixed upload/log delivery fails closed when required native files are not in this log;
it does not declare an incomplete log package to be a complete CI delivery.
"""
import argparse
import datetime
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import time
import zipfile

MiB = 1024 * 1024
MAX_ZIP = 768 * MiB
MAX_ZIP_EXPANDED = 1536 * MiB
MAX_LOG = 512 * MiB
MAX_SELECTED = 256 * MiB
MAX_RUN_DISK = 2048 * MiB
BASE = Path(__file__).resolve().parent
MARKER = b'ZIL_CI_EVIDENCE_V1 '


def consumer256_small(path):
    """Only retain source, reports and actual observations; all blobs are verified separately."""
    if not path.startswith('native-evidence/') or not any(
            part.startswith('consumer256-') for part in PurePosixPath(path).parts):
        return False
    name = PurePosixPath(path).name
    return (name in {'report.json', 'expected.json', 'stdin', 'stdout', 'stderr', 'exit', 'exit.txt'}
            or name.endswith('.zil'))


class AuditError(Exception):
    def __init__(self, code, message):
        self.code = code
        super().__init__(message)


def need(ok, message, code='EvidenceMismatch'):
    if not ok:
        raise AuditError(code, message)


def sha(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args])


def api(endpoint, output):
    data = subprocess.check_output(['gh', 'api', endpoint], timeout=90)
    need(len(data) <= 64 * MiB, 'API metadata exceeds bound')
    obj = json.loads(data)
    write_json(output, obj)
    return obj


def download(endpoint, path):
    with path.open('xb') as f:
        p = subprocess.Popen(['gh', 'api', endpoint], stdout=f, stderr=subprocess.PIPE)
        start = time.monotonic()
        try:
            while p.poll() is None:
                need(path.stat().st_size <= MAX_ZIP, 'Log ZIP exceeds download bound')
                need(time.monotonic() - start < 300, 'Log ZIP download timed out')
                time.sleep(0.2)
            stderr = p.stderr.read().decode(errors='replace')
            need(p.returncode == 0, 'Log download failed: ' + stderr[:1000], 'DownloadFailure')
        finally:
            if p.poll() is None:
                p.kill()
            p.wait()
            p.stderr.close()
    need(0 < path.stat().st_size <= MAX_ZIP, 'Invalid log ZIP size')


def safe_relative(name):
    p = PurePosixPath(name)
    need(bool(name) and not p.is_absolute() and p.as_posix() == name
         and all(x not in ('.', '..') for x in p.parts)
         and not any(x in name for x in ('\\', '\0', '\r', '\n')),
         'Unsafe archive path: ' + repr(name))
    return p


def select_transport(zip_path, out):
    """Validate every ZIP CRC. Deduplicate aggregate/step views of identical records."""
    entries = []
    candidates = []
    with zipfile.ZipFile(zip_path) as z:
        infos = z.infolist()
        need(len(infos) <= 10000, 'Too many log ZIP members')
        need(len({i.filename for i in infos}) == len(infos), 'Duplicate log ZIP names')
        need(sum(i.file_size for i in infos) <= MAX_ZIP_EXPANDED, 'Expanded logs exceed bound')
        for info in infos:
            safe_relative(info.filename.rstrip('/'))
            need(info.file_size <= MAX_LOG, 'Individual log exceeds bound')
            full = hashlib.sha256()
            records = hashlib.sha256()
            begins, ends, lines = 0, 0, 0
            # Reading to EOF checks each ZIP member CRC, including unrelated logs.
            with z.open(info) as f:
                for line in f:
                    full.update(line)
                    pos = line.find(MARKER)
                    if pos < 0:
                        continue
                    record = line[pos:].rstrip(b'\r\n')
                    if not re.match(rb'ZIL_CI_EVIDENCE_V1 (BEGIN|CHUNK|END) ', record):
                        continue
                    records.update(record + b'\n')
                    begins += record.startswith(MARKER + b'BEGIN ')
                    ends += record.startswith(MARKER + b'END ')
                    lines += 1
            entry = {'name': info.filename, 'bytes': info.file_size, 'crc32': info.CRC,
                     'sha256': full.hexdigest(), 'begin': begins, 'end': ends}
            entries.append(entry)
            if begins or ends:
                need(begins == ends == 1, 'Malformed or multiple transport envelopes in ' + info.filename)
                candidates.append({**entry, 'records_sha256': records.hexdigest(), 'records': lines})
        write_json(out / 'log-zip-members.json', entries)
        write_json(out / 'log-zip-validation.json', {
            'status': 'Pass', 'scope': 'Every ZIP member read to EOF; CRC and size bounds',
            'zip_sha256': sha(zip_path), 'members': len(entries),
            'complete_envelope_views': len(candidates)})
        need(candidates, 'Complete logs contain no transport BEGIN/END envelope', 'MissingCompleteTransport')
        need(len({c['records_sha256'] for c in candidates}) == 1,
             'Different transport streams exist in this run ZIP', 'AmbiguousTransport')
        # The job aggregate duplicates the step log; choose the smallest identical view.
        selected = min(candidates, key=lambda c: (c['bytes'], c['name']))
        log = out / 'transport.log'
        with z.open(selected['name']) as source, log.open('xb') as dest:
            shutil.copyfileobj(source, dest, 1024 * 1024)
    return log, {'zip_sha256': sha(zip_path), 'zip_bytes': zip_path.stat().st_size,
                 'zip_crc': 'Pass', 'members': len(entries), 'selected': selected,
                 'identical_transport_views': len(candidates)}


def pin_transport(repo, expected, out):
    path = out / 'evidence_transport.py'
    path.write_bytes(git(repo, 'show', expected + ':tools/ci/evidence_transport.py'))
    spec = importlib.util.spec_from_file_location('pinned_transport', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module, path


def select_small_files(archive, index, out):
    """One streaming pass; native payloads remain in the verified compressed archive."""
    chosen = [e for e in index['files'] if e['path'].startswith('receipts/')
              or (e['path'].startswith('consumer209/')
                  and (e['path'] == 'consumer209/summary.json'
                       or e['path'].endswith(('/record.json', '/stdout', '/stderr'))))
              or e['path'] == 'llvm-23.1.4/install-receipt.json'
              or consumer256_small(e['path'])]
    need(sum(e['bytes'] for e in chosen) <= MAX_SELECTED, 'Selected receipts exceed disk budget')
    by_hash = {}
    for e in chosen:
        by_hash.setdefault(e['sha256'], []).append(e)
    target_root = out / 'selected'
    target_root.mkdir()
    p = subprocess.Popen(['zstd', '-q', '-d', '--memory=64MB', '-c', str(archive)], stdout=subprocess.PIPE)
    found = set()
    try:
        with tarfile.open(fileobj=p.stdout, mode='r|', ignore_zeros=True) as tf:
            for member in tf:
                if not member.name.startswith('objects/'):
                    continue
                digest = member.name[8:]
                if digest not in by_hash:
                    continue
                need(digest not in found, 'Duplicate selected object')
                found.add(digest)
                entries = by_hash[digest]
                need(member.isfile() and member.size == entries[0]['bytes'], 'Selected object mismatch')
                primary = target_root / entries[0]['path']
                primary.parent.mkdir(parents=True, exist_ok=True)
                with tf.extractfile(member) as source, primary.open('xb') as dest:
                    shutil.copyfileobj(source, dest, 1024 * 1024)
                need(sha(primary) == digest, 'Selected object SHA mismatch')
                for e in entries[1:]:
                    other = target_root / e['path']
                    other.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(primary, other)
        while p.stdout.read(1024 * 1024):
            pass
        need(p.wait() == 0 and found == by_hash.keys(), 'Selected extraction incomplete')
    finally:
        if p.poll() is None:
            p.kill()
        p.stdout.close()
        p.wait()
    return target_root


def git_inputs(repo, expected):
    result = {}
    for row in git(repo, 'ls-tree', '-rlz', expected).split(b'\0'):
        if not row:
            continue
        meta, raw_path = row.split(b'\t', 1)
        mode, kind, oid, size = meta.decode().split()
        name = raw_path.decode()
        if name.startswith('evidence/'):
            continue
        need(kind == 'blob' and mode in ('100644', '100755', '120000'), 'Unsupported Git input')
        result[name] = {'git_mode': mode, 'git_oid': oid, 'bytes': int(size)}
    return result


def verify_source(selected, repo, expected, checkout):
    receipt = json.loads((selected / 'receipts/source-inputs.json').read_text())
    need(receipt['status'] == 'Pass' and receipt['commit'] == checkout, 'Source snapshot identity/status mismatch')
    need(receipt['checkout_before'] == receipt['checkout_after'] == 'ActualBytesTypesAndGitModesMatchHEAD',
         'Source snapshot did not verify actual checkout')
    expected_files = git_inputs(repo, expected)
    files = receipt['files']
    need(len(files) == len({f['path'] for f in files}) == len(expected_files), 'Source list duplicate/count mismatch')
    by_name = {f['path']: f for f in files}
    need(by_name.keys() == expected_files.keys(), 'Source list is not all non-evidence Git files')
    object_format = git(repo, 'rev-parse', '--show-object-format').decode().strip()
    need(receipt['git_object_format'] == object_format, 'Git object format mismatch')
    for name, expected_entry in expected_files.items():
        need(all(by_name[name][k] == v for k, v in expected_entry.items()), 'Source Git identity mismatch: ' + name)
    arc = selected / 'receipts/source-inputs.tar.gz'
    need(arc.stat().st_size == receipt['archive_bytes'] and sha(arc) == receipt['archive_sha256'],
         'Source archive byte count/SHA mismatch')
    seen = set()
    logical = 0
    with gzip.open(arc, 'rb') as gz:
        with tarfile.open(fileobj=gz, mode='r|', ignore_zeros=True) as tf:
            for m in tf:
                safe_relative(m.name.rstrip('/'))
                if m.isdir():
                    continue
                need(m.name in by_name and m.name not in seen, 'Unexpected/duplicate source archive member')
                seen.add(m.name)
                e = by_name[m.name]
                if e['git_mode'] == '120000':
                    need(m.issym(), 'Expected source symlink')
                    data = m.linkname.encode()
                else:
                    need(m.isfile() and m.size == e['bytes'] and m.size <= MAX_SELECTED,
                         'Source regular type/size mismatch')
                    need(m.mode == (0o755 if e['git_mode'] == '100755' else 0o644), 'Source executable mode mismatch')
                    need(bool(e['worktree_mode'] & 0o100) == (e['git_mode'] == '100755'), 'Recorded worktree executable mode mismatch')
                    data = tf.extractfile(m).read()
                logical += len(data)
                need(logical <= MAX_SELECTED, 'Source input expansion exceeds bound')
                need(len(data) == e['bytes'] and hashlib.sha256(data).hexdigest() == e['sha256'],
                     'Source bytes mismatch: ' + m.name)
                oid = hashlib.new(object_format, b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
                need(oid == e['git_oid'], 'Source Git blob mismatch: ' + m.name)
        # Reading gzip to EOF checks its CRC/footer independently of the outer archive.
        while data := gz.read(1024 * 1024):
            need(not any(data), 'Nonzero trailing source tar bytes')
    need(seen == by_name.keys() and receipt['input_files'] == len(seen)
         and receipt['input_blob_bytes'] == logical, 'Source snapshot incomplete')
    need((selected / 'receipts/commit.txt').read_text().strip() == checkout, 'commit.txt differs')
    # Compare full source-tree inventory too, including historical evidence identities.
    need((selected / 'receipts/source-tree.txt').read_bytes() == git(repo, 'ls-tree', '-rl', expected),
         'Full source tree receipt differs from expected Git tree')
    return {'files': len(seen), 'bytes': logical, 'archive_sha256': sha(arc), 'gzip_crc': 'Pass',
            'full_non_evidence_set': 'Pass', 'git_blobs_modes_sha256': 'Pass'}


def frozen_consumer_sources(repo, expected):
    """Read pinned Git blobs, whose full set was checked against source_snapshot above."""
    names = git(repo, 'ls-tree', '-rz', '--name-only', expected, '--', 'tests/impl256/').split(b'\0')
    files = {name.decode(): git(repo, 'show', expected + ':' + name.decode()) for name in names if name}
    need('tests/impl256/ORACLE.json' in files, 'Pinned consumer256 ORACLE missing')
    return files, json.loads(files['tests/impl256/ORACLE.json'])


def check_expected_counts(actual, expected, label):
    need(isinstance(expected, dict) and expected, 'Missing explicit expected counts: ' + label,
         'MissingExpectedCounts')
    for key, value in expected.items():
        need(key in actual and actual[key] == value,
             'Expected count mismatch: ' + label + '/' + key)


def verify_forward_consumers(index, selected, repo, expected, expectations):
    from consumer256 import verify
    source_files, oracle = frozen_consumer_sources(repo, expected)
    result = {}
    for phase in ('workspace', 'reference'):
        expected_counts = expectations['consumer256'].get(phase)
        need(isinstance(expected_counts, dict)
             and expected_counts.get('backend') in ('llvm-api', 'clang'),
             'Expected consumer phase needs explicit backend: ' + phase, 'MissingExpectedCounts')
        result[phase] = verify(index['files'], lambda name: (selected / name).read_bytes(),
                               oracle, source_files, 'native-evidence/' + phase + '/',
                               expected_counts)
    supplementary = {}
    requested = expectations.get('required_supplements', [])
    need(isinstance(requested, list) and len(requested) == len(set(requested))
         and set(requested).issubset({'consumer256-carrier'}),
         'Unknown/duplicate required supplement', 'SupplementNotAudited')
    supplement_path = 'tests/impl256/supplement/ORACLE.json'
    if supplement_path in source_files:
        need('consumer256-carrier' in requested,
             'Pinned candidate includes carrier supplement but expected coverage omits it',
             'MissingExpectedCounts')
        from consumer256_supplement import verify as verify_supplement
        supplement_oracle = json.loads(source_files[supplement_path])
        supplemental_counts = expectations.get('supplements', {}).get('consumer256-carrier', {})
        for phase in ('workspace', 'reference'):
            expected_counts = supplemental_counts.get(phase)
            need(isinstance(expected_counts, dict)
                 and expected_counts.get('backend') in ('llvm-api', 'clang'),
                 'Missing explicit carrier supplement phase counts/backend: ' + phase,
                 'MissingExpectedCounts')
            supplementary[phase] = verify_supplement(
                index['files'], lambda name: (selected / name).read_bytes(),
                supplement_oracle, source_files, 'native-evidence/' + phase + '/', expected_counts)
    else:
        need(not requested, 'Required supplement frozen sources are absent', 'SupplementNotAudited')
    return {'consumer256': result,
            'source_binding': 'Pinned Git ORACLE/source bytes; exact full source_snapshot already verified',
            'oracle_sha256': hashlib.sha256(source_files['tests/impl256/ORACLE.json']).hexdigest(),
            'supplements': {'consumer256-carrier': supplementary,
                            'counts_combined_with_consumer256': False}}


def verify_coverage(index, selected, checkout):
    files = {e['path']: e for e in index['files']}
    required = ['commit.txt', 'source-tree.txt', 'source-inputs.json', 'source-inputs.tar.gz',
                'source-restore.txt', 'toolchain.txt', 'llvm-api-toolchain.json', 'llvm-install.json',
                'fmt.log', 'workspace.log', 'reference.log', 'clippy.log', 'text-reference.log',
                'migration-runner.log', 'consumer209.json', 'workspace-native-manifest.json',
                'reference-native-manifest.json', 'disk-space-before-build.json']
    need(not index.get('missing_roots'), 'Recovery package has missing roots; retrieve successful primary artifacts separately',
         'IncompleteEvidenceCoverage')
    for name in required:
        need('receipts/' + name in files, 'Required receipt missing: ' + name, 'IncompleteEvidenceCoverage')
    native = {}
    for phase in ('workspace', 'reference'):
        manifest = json.loads((selected / f'receipts/{phase}-native-manifest.json').read_text())
        need(manifest['phase'] == phase and manifest['files'], 'Empty/wrong native manifest')
        expected_entries = manifest['files']
        need(len(expected_entries) == len({e['path'] for e in expected_entries}), 'Duplicate native manifest paths')
        prefix = f'native-evidence/{phase}/'
        actual = {n[len(prefix):]: e for n, e in files.items() if n.startswith(prefix)}
        need(actual.keys() == {e['path'] for e in expected_entries},
             'Native payload set differs from manifest: ' + phase, 'IncompleteEvidenceCoverage')
        for e in expected_entries:
            need(all(actual[e['path']][key] == e[key] for key in ('bytes', 'mode', 'sha256')),
                 'Native manifest mismatch: ' + e['path'])
        native[phase] = {'files': len(actual), 'logical_bytes': sum(e['bytes'] for e in actual.values())}
    consumer = json.loads((selected / 'consumer209/summary.json').read_text())
    need(consumer == json.loads((selected / 'receipts/consumer209.json').read_text()), 'Consumer summary copies differ')
    need(consumer['status'] == 'Pass' and consumer['source_commit'] == checkout
         and consumer['builds'] and consumer['observations'], 'Consumer status/identity/coverage mismatch')
    for b in consumer['builds']:
        name = 'consumer209/' + b['name'] + '-program'
        need(name in files and files[name]['sha256'] == b['executable_sha256'], 'Consumer executable missing/mismatched')
        need(name + '.ll' in files and files[name + '.ll']['sha256'] == b['llvm_sha256'], 'Consumer LLVM input missing/mismatched')
        need(b['verification_runs'] == 1, 'Consumer independent verification absent')
        for suffix in ('record.json', 'stdout', 'stderr'):
            need('consumer209/' + b['name'] + '/build/' + suffix in files, 'Consumer build original missing')
    command_records = [n for n in files if n.startswith('consumer209/') and n.endswith('/record.json')]
    need(len(command_records) == consumer['commands'], 'Consumer command originals count mismatch')
    for name in command_records:
        directory = name.rsplit('/', 1)[0]
        for suffix in ('stdout', 'stderr'):
            need(directory + '/' + suffix in files, 'Consumer command output missing: ' + directory)
    for obs in consumer['observations']:
        directory = 'consumer209/' + obs['case'] + '-fail' + str(obs['fail_at'])
        for suffix in ('record.json', 'stdout', 'stderr'):
            need(directory + '/' + suffix in files, 'Consumer observation original missing: ' + directory)
        command = json.loads((selected / directory / 'record.json').read_text())
        need(command.get('returncode') == obs['exit'] and not command.get('timeout'),
             'Consumer observation return code mismatch')
        need((selected / directory / 'stdout').read_bytes() == obs['stdout'].encode(),
             'Consumer observation stdout differs from summary')
        stderr = (selected / directory / 'stderr').read_bytes()
        need((json.loads(stderr) == obs['allocation']) if obs['allocation'] is not None else not stderr,
             'Consumer allocation/stderr differs from summary')
    for name, key in [('allocator.o', 'probe_sha256'), ('link-probe.py', 'link_wrapper_sha256')]:
        need('consumer209/' + name in files and files['consumer209/' + name]['sha256'] == consumer[key],
             'Consumer probe/wrapper missing or mismatched')
    for suffix in ('record.json', 'stdout', 'stderr'):
        need('consumer209/prepare-probe/' + suffix in files, 'Consumer probe preparation original missing')
    need('consumer209/probe-link-argv.jsonl' in files, 'Consumer link invocation log absent')
    counts = {}
    for name in ('workspace.log', 'reference.log', 'text-reference.log'):
        rows = re.findall(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',
                          (selected / 'receipts' / name).read_text())
        need(rows and all(r[0] == 'ok' and r[2] == r[3] == '0' for r in rows), 'Failed/ignored/no test results: ' + name)
        counts[name] = {'passed': sum(int(r[1]) for r in rows), 'failed': 0, 'ignored': 0, 'groups': len(rows)}
    if 'toolchain_checkpoint' in index.get('uploads', {}):
        identity = json.loads((selected / 'receipts/llvm-research-identity.json').read_text())
        need(identity['status'] == 'Pass', 'Research LLVM identity not Pass')
        need(any(n.startswith('receipts/llvm-research-evidence/') for n in files), 'Research toolchain source receipts absent')
    return {'native': native, 'counts': counts, 'consumer_builds': len(consumer['builds']),
            'consumer_observations': len(consumer['observations']), 'consumer_command_originals': len(command_records)}


def audit(args, result, out):
    repo = args.repo.resolve()
    need(args.expectations is not None, 'Formal audit requires root-frozen expected counts', 'MissingExpectedCounts')
    expectations = json.loads(args.expectations.read_text())
    need('test_counts' in expectations and 'consumer256' in expectations,
         'Expectations need test_counts and consumer256 phase maps', 'MissingExpectedCounts')
    result['expectations'] = {'path': str(args.expectations), 'sha256': sha(args.expectations)}
    need(re.fullmatch(r'[0-9a-f]{40}', args.expected_head), 'Expected head must be full commit SHA')
    need(shutil.disk_usage(out).free >= 3 * 1024**3, 'Need at least 3 GiB free before audit', 'InsufficientSpace')
    root = f'repos/{args.remote}/actions/runs/{args.run_id}'
    run = api(root, out / 'run.json')
    need(str(run['id']) == args.run_id and run['status'] == 'completed', 'Run is not the requested completed run')
    need(run['head_sha'] == args.expected_head, 'Run head differs from expected candidate')
    attempt = run['run_attempt']
    jobs = api(root + f'/attempts/{attempt}/jobs?per_page=100', out / 'jobs.json')
    need(jobs['total_count'] == len(jobs['jobs']) == 1, 'Expected one job; refuse ambiguous/missing jobs')
    job = jobs['jobs'][0]
    result.update(run_attempt=attempt, job_id=job['id'], run_conclusion=run['conclusion'],
                  job_conclusion=job['conclusion'], expected_head=args.expected_head)
    zip_path = out / 'logs.zip'
    download(root + f'/attempts/{attempt}/logs', zip_path)
    result['log_zip'] = {'bytes': zip_path.stat().st_size, 'sha256': sha(zip_path)}
    log, result['transport_log'] = select_transport(zip_path, out)
    transport, producer = pin_transport(repo, args.expected_head, out)
    archive = out / 'evidence.tar.zst'
    result['recovery_header'] = transport.recover(log, archive)
    index = transport.verify(archive)
    write_json(out / 'INDEX.json', index)
    result['archive'] = {'bytes': archive.stat().st_size, 'sha256': sha(archive),
                         'files': len(index['files']), 'logical_bytes': sum(e['bytes'] for e in index['files'])}
    need(str(index['run_id']) == args.run_id and str(index['run_attempt']) == str(attempt), 'INDEX run/attempt mismatch')
    need(index['producer_sha256'] == sha(producer), 'Producer differs from expected candidate')
    checkout = index['checkout']
    need(isinstance(checkout, str) and re.fullmatch(r'[0-9a-f]{40}', checkout), 'Invalid checkout identity')
    actual = api(f'repos/{args.remote}/git/commits/{checkout}', out / 'actual-checkout.json')
    expected_tree = git(repo, 'rev-parse', args.expected_head + '^{tree}').decode().strip()
    need(actual['tree']['sha'] == expected_tree, 'CI checkout tree differs from candidate')
    result.update(actual_checkout=checkout, tree=expected_tree, uploads=index['uploads'])
    selected = select_small_files(archive, index, out)
    result['source'] = verify_source(selected, repo, args.expected_head, checkout)
    result['coverage'] = verify_coverage(index, selected, checkout)
    for phase in ('workspace.log', 'reference.log', 'text-reference.log'):
        check_expected_counts(result['coverage']['counts'][phase], expectations['test_counts'].get(phase), phase)
    result['forward_consumers'] = verify_forward_consumers(index, selected, repo, args.expected_head, expectations)
    # Upload outcomes are deliberately distinct from successful compiler gates.
    required_steps = ['Formatting', 'Complete workspace tests', 'Reference-only build and explicit unavailable control',
                      'Native API State and allocation-failure controls', 'All-target Clippy',
                      'Explicit LLVM text-reference boundary', 'Test CI evidence recovery',
                      'Recover complete evidence through verified log transport', 'Report every evidence upload outcome']
    by_name = {s['name']: s for s in job['steps']}
    for name in required_steps:
        need(name in by_name and by_name[name]['conclusion'] == 'success', 'Required CI step not successful: ' + name)
    need(run['conclusion'] == job['conclusion'] == 'success', 'Overall workflow did not complete successfully')
    result['disk_bytes'] = sum(p.stat().st_size for p in out.rglob('*') if p.is_file())
    need(result['disk_bytes'] <= MAX_RUN_DISK, 'Audit output exceeds per-run disk budget')
    result['status'] = 'Pass'
    result['scope'] = 'Exact-run delivery completeness and recorded successful gates; no new compiler execution or product-performance claim.'


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('run_id')
    p.add_argument('expected_head')
    p.add_argument('--repo', type=Path, default=Path('/workspace/zil-core'))
    p.add_argument('--remote', default='zil-lang/zil')
    p.add_argument('--output', type=Path)
    p.add_argument('--expectations', type=Path,
                   help='Root-frozen exact test and per-phase consumer counts, supplied after formal R checks')
    args = p.parse_args()
    need(args.run_id.isdigit(), 'Run ID must be numeric')
    out = (args.output or BASE / ('run-' + args.run_id)).resolve()
    need(out.is_relative_to(BASE) and out != BASE, 'Output must be a new subdirectory of ci-audit')
    out.mkdir(parents=True, exist_ok=False)
    result = {'status': 'Running', 'run_id': args.run_id, 'auditor': '/root/assess_delivery_process',
              'script_sha256': sha(Path(__file__)), 'started_at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}
    try:
        audit(args, result, out)
    except Exception as exc:
        result.update(status='Fail', failure_code=getattr(exc, 'code', type(exc).__name__), error=str(exc))
    finally:
        result['finished_at_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        write_json(out / 'RESULT.json', result)
        print(json.dumps({'status': result['status'], 'result': str(out / 'RESULT.json'),
                          'failure_code': result.get('failure_code'), 'error': result.get('error')}))
    return 0 if result['status'] == 'Pass' else 1


if __name__ == '__main__':
    sys.exit(main())

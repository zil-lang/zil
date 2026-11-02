#!/usr/bin/env python3
"""Independent bounded fixtures for verify_coverage; no CI or compiler execution."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location('reviewed_audit', ROOT / 'audit_run.py')
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)
CHECKOUT = 'a' * 40


def sha(data):
    return hashlib.sha256(data).hexdigest()


class Fixture:
    def __init__(self, root):
        self.root = root
        self.entries = {}
        self.summary = {
            'status': 'Pass', 'source_commit': CHECKOUT,
            'builds': [], 'observations': [], 'commands': 5,
            'probe_sha256': sha(b'probe'), 'link_wrapper_sha256': sha(b'wrapper'),
        }
        required = ['commit.txt', 'source-tree.txt', 'source-inputs.json',
                    'source-inputs.tar.gz', 'source-restore.txt', 'toolchain.txt',
                    'llvm-api-toolchain.json', 'llvm-install.json', 'fmt.log',
                    'clippy.log', 'migration-runner.log', 'disk-space-before-build.json']
        for name in required:
            self.put('receipts/' + name, b'coverage-only placeholder\n')
        for name in ('workspace.log', 'reference.log', 'text-reference.log'):
            self.put('receipts/' + name,
                     b'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n')
        for phase in ('workspace', 'reference'):
            entry = self.put(f'native-evidence/{phase}/program', phase.encode())
            self.json(f'receipts/{phase}-native-manifest.json',
                      {'phase': phase, 'files': [{**entry, 'path': 'program'}]})
        self.put('consumer209/allocator.o', b'probe')
        self.put('consumer209/link-probe.py', b'wrapper')
        self.put('consumer209/probe-link-argv.jsonl', b'["clang"]\n')
        self.command('consumer209/prepare-probe', 0, b'', b'')
        for mode in ('ordinary', 'probe'):
            name = 'u1/main/O0/' + mode
            exe = ('exe-' + mode).encode()
            llvm = ('llvm-' + mode).encode()
            self.put('consumer209/' + name + '-program', exe)
            self.put('consumer209/' + name + '-program.ll', llvm)
            self.command('consumer209/' + name + '/build', 0, b'{}', b'')
            self.summary['builds'].append({'name': name, 'executable_sha256': sha(exe),
                'llvm_sha256': sha(llvm), 'verification_runs': 1})
            allocation = None if mode == 'ordinary' else {
                'live': 0, 'allocated': 1, 'frees': 1, 'failures': 0}
            case = name + '/sample'
            stderr = b'' if allocation is None else json.dumps(allocation).encode()
            self.command('consumer209/' + case + '-fail0', 0, b'OK\n', stderr)
            self.summary['observations'].append({'case': case, 'fail_at': 0,
                'exit': 0, 'stdout': 'OK\n', 'allocation': allocation})
        self.sync_summary()

    def put(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
        entry = {'path': name, 'bytes': len(content), 'mode': 0o644, 'sha256': sha(content)}
        self.entries[name] = entry
        return entry

    def json(self, name, value):
        return self.put(name, json.dumps(value).encode())

    def command(self, name, code, stdout, stderr):
        self.json(name + '/record.json', {'returncode': code, 'argv': ['fixture']})
        self.put(name + '/stdout', stdout)
        self.put(name + '/stderr', stderr)

    def drop(self, name):
        del self.entries[name]
        (self.root / name).unlink()

    def sync_summary(self):
        self.json('consumer209/summary.json', self.summary)
        self.json('receipts/consumer209.json', self.summary)

    def verify(self):
        return AUDIT.verify_coverage({'files': list(self.entries.values()),
                                    'missing_roots': [], 'uploads': {}}, self.root, CHECKOUT)


def main():
    ordinary = 'consumer209/u1/main/O0/ordinary/sample-fail0/'
    probe = 'consumer209/u1/main/O0/probe/sample-fail0/'
    cases = [
        ('complete_consumer_control', None, None),
        ('missing_observation_stdout', lambda f: f.drop(ordinary + 'stdout'), 'Consumer command output missing'),
        ('missing_observation_stderr', lambda f: f.drop(ordinary + 'stderr'), 'Consumer command output missing'),
        ('missing_observation_record', lambda f: f.drop(ordinary + 'record.json'), 'Consumer command originals count mismatch'),
        ('missing_prepare_probe_stdout', lambda f: f.drop('consumer209/prepare-probe/stdout'), 'Consumer command output missing'),
        ('missing_prepare_probe_record', lambda f: f.drop('consumer209/prepare-probe/record.json'), 'Consumer command originals count mismatch'),
        ('missing_probe_object', lambda f: f.drop('consumer209/allocator.o'), 'Consumer probe/wrapper missing or mismatched'),
        ('missing_wrapper', lambda f: f.drop('consumer209/link-probe.py'), 'Consumer probe/wrapper missing or mismatched'),
        ('mismatched_wrapper', lambda f: f.put('consumer209/link-probe.py', b'different'), 'Consumer probe/wrapper missing or mismatched'),
        ('missing_link_invocations', lambda f: f.drop('consumer209/probe-link-argv.jsonl'), 'Consumer link invocation log absent'),
        ('wrong_observation_stdout', lambda f: f.put(ordinary + 'stdout', b'WRONG\n'), 'Consumer observation stdout differs from summary'),
        ('wrong_observation_returncode', lambda f: f.json(ordinary + 'record.json', {'returncode': 1}), 'Consumer observation return code mismatch'),
        ('observation_timeout', lambda f: f.json(ordinary + 'record.json', {'returncode': 0, 'timeout': True}), 'Consumer observation return code mismatch'),
        ('wrong_observation_allocation', lambda f: f.json(probe + 'stderr', {'live': 1}), 'Consumer allocation/stderr differs from summary'),
        ('unexpected_ordinary_stderr', lambda f: f.put(ordinary + 'stderr', b'bad'), 'Consumer allocation/stderr differs from summary'),
    ]
    results = []
    with tempfile.TemporaryDirectory(prefix='zil-consumer-audit-review-') as temporary:
        for name, mutation, expected_error in cases:
            f = Fixture(Path(temporary) / name)
            if mutation:
                mutation(f)
            try:
                detail = f.verify()
                passed = expected_error is None
                results.append({'case': name, 'status': 'Pass' if passed else 'Fail',
                                'observed': 'Accepted', 'detail': detail})
            except Exception as error:
                passed = isinstance(error, AUDIT.AuditError) and expected_error is not None and expected_error in str(error)
                results.append({'case': name, 'status': 'Pass' if passed else 'Fail',
                                'observed': type(error).__name__, 'message': str(error),
                                'expected_message': expected_error})
    report = {'status': 'Pass' if all(r['status'] == 'Pass' for r in results) else 'Fail',
              'reviewer': '/root/assess_delivery_process/audit_transport_limits',
              'scope': 'Bounded synthetic verify_coverage consumer fixtures only; no CI download, source audit, compiler or performance execution.',
              'audit_script_sha256': sha((ROOT / 'audit_run.py').read_bytes()),
              'test_script_sha256': sha(Path(__file__).read_bytes()),
              'cases': len(results), 'passed': sum(r['status'] == 'Pass' for r in results),
              'results': results}
    (ROOT / 'transport-review.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ('status', 'cases', 'passed', 'audit_script_sha256')}))
    return 0 if report['status'] == 'Pass' else 1


if __name__ == '__main__':
    raise SystemExit(main())

import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile

import audit_run as audit

sys.dont_write_bytecode = True


class AuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='test-', dir=audit.BASE)
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def zip(self, members):
        path = self.root / 'logs.zip'
        with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_STORED) as z:
            for name, data in members:
                z.writestr(name, data)
        return path

    def test_aggregate_and_step_same_transport_are_one_view(self):
        stream = b'ZIL_CI_EVIDENCE_V1 BEGIN {}\nZIL_CI_EVIDENCE_V1 CHUNK 0 YQ==\nZIL_CI_EVIDENCE_V1 END abc\n'
        path = self.zip([('0_workspace.txt', b'unrelated\n' + stream), ('workspace/30_recovery.txt', stream)])
        log, result = audit.select_transport(path, self.root)
        self.assertEqual(log.read_bytes(), stream)
        self.assertEqual(result['identical_transport_views'], 2)
        self.assertEqual(result['selected']['name'], 'workspace/30_recovery.txt')

    def test_conflicting_transport_views_rejected(self):
        stream = b'ZIL_CI_EVIDENCE_V1 BEGIN {}\nZIL_CI_EVIDENCE_V1 END abc\n'
        path = self.zip([('a.txt', stream), ('b.txt', stream.replace(b'abc', b'xyz'))])
        with self.assertRaisesRegex(audit.AuditError, 'Different transport'):
            audit.select_transport(path, self.root)

    def test_absent_transport_and_corrupt_zip_rejected(self):
        path = self.zip([('job.txt', b'Only successful tests, no native transport\n')])
        with self.assertRaises(audit.AuditError) as failure:
            audit.select_transport(path, self.root)
        self.assertEqual(failure.exception.code, 'MissingCompleteTransport')
        raw = path.read_bytes().replace(b'Only successful tests', b'Only corrupted! tests')
        path.write_bytes(raw)
        with self.assertRaises(zipfile.BadZipFile):
            audit.select_transport(path, self.root)

    def fixture(self):
        repo = self.root / 'repo'
        repo.mkdir()
        subprocess.run(['git', 'init', '-q', str(repo)], check=True)
        (repo / 'source.zil').write_text('fn main() -> i32 { 42 }\n')
        (repo / 'helper.sh').write_text('#!/bin/sh\nexit 0\n')
        (repo / 'helper.sh').chmod(0o755)
        (repo / 'alias').symlink_to('source.zil')
        (repo / 'evidence').mkdir()
        (repo / 'evidence/history.json').write_text('{"status":"Fail"}\n')
        subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
        subprocess.run(['git', '-C', str(repo), '-c', 'user.name=Audit Fixture', '-c',
                        'user.email=audit@example.invalid', 'commit', '-qm', 'fixture'], check=True)
        commit = audit.git(repo, 'rev-parse', 'HEAD').decode().strip()
        spec = importlib.util.spec_from_file_location('source_snapshot', '/workspace/zil-core/tools/ci/source_snapshot.py')
        snapshot = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(snapshot)
        selected = self.root / 'selected'
        snapshot.capture(repo, selected / 'receipts')
        return repo, selected, commit

    def test_complete_git_bound_source_snapshot_including_modes_and_symlink(self):
        repo, selected, commit = self.fixture()
        result = audit.verify_source(selected, repo, commit, commit)
        self.assertEqual(result['files'], 3)
        self.assertEqual(result['gzip_crc'], 'Pass')

    def test_self_consistent_source_list_omission_is_rejected(self):
        repo, selected, commit = self.fixture()
        receipt = selected / 'receipts/source-inputs.json'
        data = json.loads(receipt.read_text())
        data['files'].pop()
        data['input_files'] -= 1
        receipt.write_text(json.dumps(data))
        with self.assertRaisesRegex(audit.AuditError, 'duplicate/count mismatch'):
            audit.verify_source(selected, repo, commit, commit)

    def test_source_archive_corruption_is_rejected(self):
        repo, selected, commit = self.fixture()
        arc = selected / 'receipts/source-inputs.tar.gz'
        data = bytearray(arc.read_bytes())
        data[-5] ^= 1
        arc.write_bytes(data)
        receipt = selected / 'receipts/source-inputs.json'
        meta = json.loads(receipt.read_text())
        meta['archive_sha256'] = audit.sha(arc)
        receipt.write_text(json.dumps(meta))
        with self.assertRaises((OSError, EOFError)):
            audit.verify_source(selected, repo, commit, commit)


if __name__ == '__main__':
    unittest.main()

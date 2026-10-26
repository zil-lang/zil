#!/usr/bin/env python3
"""Small real archives/Git repositories exercise delivery boundaries."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
import package
import package_check
import evidence_archive as audit


class DeliveryTests(unittest.TestCase):
    def setUp(self):
        self.area = tempfile.TemporaryDirectory(prefix='zil-package-test-')
        self.addCleanup(self.area.cleanup)
        self.base = Path(self.area.name)
        self.root = self.base / 'project'
        self.root.mkdir()
        for name in ['package.py', 'package_check.py', 'evidence_archive.py']:
            (self.root / 'tools').mkdir(exist_ok=True)
            shutil.copy2(ROOT / 'tools' / name, self.root / 'tools' / name)
        self.put('main.rs', 'fn main() {}\n')
        self.put('.gitignore', '/out/\n__pycache__/\n')
        self.put('.task/impl/001.md', '```json\n' + json.dumps({'evidence': ['evidence/impl/001/acceptance.json']}) + '\n```\n')
        self.put('evidence/impl/001/acceptance.json', '{"status":"Pass","executed":1}\n')
        self.put('evidence/impl/001/before.log', 'original failure\n')
        self.put('evidence/run/corpus/example/case.json', '{"actions":["edit","check"]}\n')
        self.put('evidence/run/corpus/example/reduced-one.json', '{"actions":["check"]}\n')
        self.put('evidence/raw/run/program', b'\x7fELF' + bytes(range(256)) * 4)
        self.put('evidence/raw/run/program-copy', (self.root / 'evidence/raw/run/program').read_bytes())
        self.put('evidence/raw/run/stage.txt', 'large stage\n' * 5000)
        self.put('evidence/raw/run/case/result.json', '{"status":"Pass"}\n')
        self.put('target/do-not-package', 'cache')
        subprocess.run(['git', 'init', '-b', 'main', str(self.root)], check=True, capture_output=True)
        package.git(self.root, 'config', 'user.name', 'Delivery Fixture')
        package.git(self.root, 'config', 'user.email', 'delivery-fixture@invalid')
        package.git(self.root, 'add', '.')
        package.git(self.root, 'commit', '-m', 'fixture')
        self.output = self.base / 'release.zip'

    def put(self, name, data):
        target = self.root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data.encode() if isinstance(data, str) else data)

    def make(self, **kwargs):
        return package.build(self.root, self.output, **kwargs)

    def unpack(self):
        with zipfile.ZipFile(self.output) as archive:
            archive.extractall(self.base / 'unpacked')
        return self.base / 'unpacked/zil-rust'

    def rewrite_audit(self, name, change):
        target = self.base / (name + '.zip')
        with zipfile.ZipFile(self.base / 'release-audit.zip') as source:
            rows = [(item, source.read(item)) for item in source.infolist()]
        rows = change(rows)
        with warnings.catch_warnings(), zipfile.ZipFile(target, 'w') as output:
            warnings.simplefilter('ignore', UserWarning)
            for item, data in rows:
                output.writestr(item, data)
        return target

    def test_real_split_restore_and_history(self):
        report = self.make()
        restored = self.unpack()
        self.assertTrue(report['ok'])
        self.assertEqual(report['audit_statistics']['native_files'], 2)
        self.assertGreaterEqual(report['audit_statistics']['duplicate_extra_files'], 1)
        self.assertFalse((restored / 'evidence/raw/run/program').exists())
        self.assertFalse((restored / 'evidence/raw/run/case/result.json').exists())
        self.assertTrue((restored / 'evidence/impl/001/before.log').exists())
        self.assertTrue((restored / 'evidence/run/corpus/example/case.json').exists())
        self.assertFalse((restored / 'target').exists())
        self.assertFalse((restored / '.git').exists())
        self.assertEqual(list(restored.rglob('*.bundle')), [])
        result = subprocess.run([sys.executable, str(restored / 'tools/package_check.py'), '--companions', str(self.base)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        raw = self.base / 'release-audit.zip'
        self.assertTrue(audit.restore(raw, restored)['ok'])
        self.assertTrue(package_check.check(restored, self.base)['ok'])
        self.assertEqual(audit.restore(raw, restored)['files_written'], 0)
        self.assertEqual((restored / 'evidence/raw/run/program').read_bytes(), (self.root / 'evidence/raw/run/program').read_bytes())
        clone = self.base / 'clone'
        subprocess.run(['git', 'clone', '--branch', 'main', str(self.base / 'release-history.bundle'), str(clone)], check=True, capture_output=True)
        self.assertEqual(package.git(clone, 'rev-parse', 'HEAD'), package.git(self.root, 'rev-parse', 'HEAD'))

    def test_source_and_companion_tampering(self):
        self.make()
        restored = self.unpack()
        (restored / 'main.rs').write_text('changed')
        self.assertFalse(package_check.check(restored)['ok'])
        (restored / 'main.rs').write_text('fn main() {}\n')
        raw = self.base / 'release-audit.zip'
        raw.write_bytes(raw.read_bytes() + b'changed')
        self.assertFalse(package_check.check(restored, self.base)['ok'])
        with self.assertRaisesRegex(ValueError, 'SHA-256 mismatch'):
            audit.restore(raw, restored)
        self.assertFalse((restored / 'AUDIT-MANIFEST.json').exists())

    def test_active_summary_survives_compact_size_limit(self):
        summary = json.dumps({
            'run_id': 'current', 'draft_gate': 'Pass',
            'result_path': 'evidence/validation/current/result.json',
            'records': [], 'detail': 'x' * (package.MAX_COMPACT + 1),
        }) + '\n'
        self.assertGreater(len(summary.encode()), package.MAX_COMPACT)
        self.put('evidence/final-summary.json', summary)
        self.put('evidence/validation/current/result.json', summary)
        self.put('evidence/validation/historical/result.json', summary)
        self.make(without_history=True)
        restored = self.unpack()
        self.assertTrue((restored / 'evidence/final-summary.json').is_file())
        self.assertEqual((restored / 'evidence/final-summary.json').read_text(), summary)
        self.assertEqual((restored / 'evidence/validation/current/result.json').read_text(), summary)
        self.assertFalse((restored / 'evidence/validation/historical/result.json').exists())
        self.assertTrue(package_check.check(restored, self.base)['ok'])

    def test_missing_and_unlisted_source_files(self):
        self.make()
        restored = self.unpack()
        (restored / 'main.rs').unlink()
        self.assertFalse(package_check.check(restored)['ok'])
        (restored / 'main.rs').write_text('fn main() {}\n')
        (restored / 'surprise.txt').write_text('extra')
        self.assertFalse(package_check.check(restored)['ok'])

    def test_duplicate_missing_and_changed_audit_entries(self):
        self.make()
        first = lambda rows: next((i, b) for i, b in rows if i.filename.endswith('/program'))
        changes = {
            'duplicate': lambda rows: rows + [first(rows)],
            'missing': lambda rows: [(i, b) for i, b in rows if not i.filename.endswith('/program')],
            'changed': lambda rows: [(i, b + b'!' if i.filename.endswith('/program') else b) for i, b in rows],
        }
        for name, change in changes.items():
            with self.subTest(name=name):
                bad = self.rewrite_audit(name, change)
                dest = self.base / ('dest-' + name)
                with self.assertRaises(ValueError):
                    audit.restore(bad, dest)
                self.assertFalse(dest.exists())

    def test_escape_noncanonical_and_symlink_entries(self):
        self.make()
        for name in ['zil-rust/evidence/../../escape', 'zil-rust/evidence//bad', 'zil-rust/evidence/C:bad', 'zil-rust/evidence\\bad']:
            with self.subTest(name=name):
                bad = self.rewrite_audit('escape', lambda rows: rows + [(zipfile.ZipInfo(name), b'bad')])
                with self.assertRaises(ValueError):
                    audit.restore(bad, self.base / 'safe')
                self.assertFalse((self.base / 'safe').exists())
        link = zipfile.ZipInfo('zil-rust/evidence/link')
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        bad = self.rewrite_audit('link', lambda rows: rows + [(link, b'../../escape')])
        with self.assertRaisesRegex(ValueError, 'ordinary files'):
            audit.inspect(bad)

    def test_restore_conflict_and_symlink_do_not_write(self):
        self.make()
        destination = self.base / 'conflict'
        conflict = destination / 'evidence/raw/run/program'
        conflict.parent.mkdir(parents=True)
        conflict.write_text('keep me')
        with self.assertRaisesRegex(ValueError, 'different existing content'):
            audit.restore(self.base / 'release-audit.zip', destination)
        self.assertEqual(conflict.read_text(), 'keep me')
        self.assertFalse((destination / 'evidence/impl').exists())
        symlink_dest = self.base / 'symlink-dest'
        symlink_dest.mkdir()
        (symlink_dest / 'evidence').symlink_to(destination, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'Symlink'):
            audit.restore(self.base / 'release-audit.zip', symlink_dest)

    def test_destination_ancestor_redirect_is_rejected_before_any_write(self):
        self.make()
        safe, outside = self.base / 'safe', self.base / 'outside'
        safe.mkdir()
        outside.mkdir()
        (safe / 'redirect').symlink_to(outside, target_is_directory=True)
        destination = safe / 'redirect' / 'restored'
        with self.assertRaisesRegex(ValueError, 'destination ancestry'):
            audit.restore(self.base / 'release-audit.zip', destination)
        self.assertEqual(list(outside.iterdir()), [])
        self.assertFalse(destination.exists())
        self.assertEqual([p.name for p in safe.iterdir()], ['redirect'])
        obstruction = safe / 'ordinary-file'
        obstruction.write_text('keep me')
        with self.assertRaisesRegex(ValueError, 'Non-directory in destination ancestry'):
            audit.restore(self.base / 'release-audit.zip', obstruction / 'restored')
        self.assertEqual(obstruction.read_text(), 'keep me')
        self.assertEqual(list(outside.iterdir()), [])

    def test_missing_required_receipt_and_unexpected_binary(self):
        receipt = self.root / 'evidence/impl/001/acceptance.json'
        original = receipt.read_bytes()
        receipt.unlink()
        with self.assertRaisesRegex(ValueError, 'Required compact evidence'):
            self.make(without_history=True)
        self.assertFalse(self.output.exists())
        receipt.write_bytes(original)
        self.put('unnecessary/program', b'\x7fELFbinary')
        with self.assertRaisesRegex(ValueError, 'Unexpected native binary'):
            self.make(without_history=True)
        self.assertFalse(self.output.exists())

    def test_dirty_history_is_rejected_and_explicit_omission_is_honest(self):
        self.put('main.rs', 'edited\n')
        with self.assertRaisesRegex(ValueError, 'clean main'):
            self.make()
        self.make(without_history=True)
        restored = self.unpack()
        manifest = json.loads((restored / 'PACKAGE.json').read_text())
        self.assertTrue(manifest['source']['dirty'])
        self.assertEqual(manifest['companions']['history']['status'], 'not-included')
        self.assertTrue(manifest['historical_handoff_requires_git'])
        self.assertFalse((self.base / 'release-history.bundle').exists())

    def test_existing_release_is_not_replaced(self):
        self.make()
        before = hashlib.sha256(self.output.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, 'already exists'):
            self.make()
        self.assertEqual(hashlib.sha256(self.output.read_bytes()).hexdigest(), before)

    def test_explicit_raw_run_and_compact_repack_boundary(self):
        self.put('out/runs/selected/bin/driver', b'\x7fELFselected')
        self.put('out/runs/unselected/bin/driver', b'\x7fELFunselected')
        self.make(raw_runs=['out/runs/selected'])
        restored = self.unpack()
        self.assertFalse((restored / 'out').exists())
        audit.restore(self.base / 'release-audit.zip', restored)
        self.assertTrue((restored / 'out/runs/selected/bin/driver').exists())
        self.assertFalse((restored / 'out/runs/unselected').exists())
        self.assertTrue(package_check.check(restored)['ok'])
        with self.assertRaisesRegex(ValueError, 'Repackage from a source checkout'):
            package.build(restored, self.base / 'new.zip', without_history=True)

    def test_pruned_tree_reuses_identical_complete_audit(self):
        self.make()
        original_audit = self.base / 'release-audit.zip'
        original_sha = package_check.digest(original_audit)
        shutil.rmtree(self.root / 'evidence/raw')
        package.git(self.root, 'add', '-u')
        package.git(self.root, 'commit', '-m', 'move raw evidence out of current tree')
        next_output = self.base / 'pruned.zip'
        result = package.build(self.root, next_output, audit_input=original_audit)
        self.assertEqual(result['audit']['sha256'], original_sha)
        self.assertGreater(result['retention']['evidence_files_external'], 0)
        with zipfile.ZipFile(next_output) as archive:
            archive.extractall(self.base / 'pruned')
        restored = self.base / 'pruned/zil-rust'
        audit.restore(self.base / 'pruned-audit.zip', restored)
        self.assertTrue(package_check.check(restored)['ok'])
        self.assertTrue((restored / 'evidence/raw/run/program').exists())
        self.put('evidence/impl/001/acceptance.json', '{"status":"Fail"}\n')
        with self.assertRaisesRegex(ValueError, 'not identical'):
            package.build(self.root, self.base / 'changed.zip', without_history=True, audit_input=original_audit)
        self.assertFalse((self.base / 'changed.zip').exists())

    def test_manifest_escape_duplicate_and_restored_tampering(self):
        self.make()
        restored = self.unpack()
        sums = restored / 'SHA256SUMS'
        original = sums.read_text()
        sums.write_text(original + '0' * 64 + '  ../outside\n')
        with self.assertRaises(ValueError):
            package_check.check(restored)
        sums.write_text(original + original.splitlines()[0] + '\n')
        with self.assertRaises(ValueError):
            package_check.check(restored)
        sums.write_text(original)
        audit.restore(self.base / 'release-audit.zip', restored)
        (restored / 'evidence/raw/run/program').write_text('tampered')
        self.assertFalse(package_check.check(restored)['ok'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(DeliveryTests))
    report = {'schema_version': 1, 'status': 'Pass' if result.wasSuccessful() else 'Fail',
              'scope': 'Packaging, real Git bundle recovery, and evidence integrity controls; no language acceptance',
              'executed': result.testsRun, 'failures': len(result.failures), 'errors': len(result.errors)}
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))
    sys.exit(0 if result.wasSuccessful() else 1)

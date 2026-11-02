#!/usr/bin/env python3
"""Check cleanup boundaries and summary accuracy without GitHub Actions."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import compact_results


class CompactResultsTests(unittest.TestCase):
    def test_cleanup_only_completed_phase(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in ('native-evidence/workspace', 'native-evidence/reference',
                         'target/debug', 'target/release', 'target-reference'):
                path = root / name
                path.mkdir(parents=True)
                (path / 'file').write_text('data')
            compact_results.cleanup(root, 'workspace')
            self.assertFalse((root / 'native-evidence/workspace').exists())
            self.assertFalse((root / 'target/debug').exists())
            for name in ('native-evidence/reference', 'target/release', 'target-reference'):
                self.assertTrue((root / name / 'file').exists())
            compact_results.cleanup(root, 'reference')
            self.assertFalse((root / 'target-reference').exists())
            self.assertTrue((root / 'target/release/file').exists())

    def test_cleanup_refuses_symlinked_parent_and_root(self):
        with tempfile.TemporaryDirectory() as tmp, tempfile.TemporaryDirectory() as other:
            root = Path(tmp)
            (root / 'native-evidence').symlink_to(other, target_is_directory=True)
            with self.assertRaises(ValueError):
                compact_results.cleanup(root, 'workspace')
            self.assertTrue(Path(other).exists())
            with self.assertRaises(ValueError):
                compact_results.cleanup(Path('/'), 'workspace')

    def test_summary_records_failure_and_all_test_counts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'receipts').mkdir()
            (root / 'receipts/workspace.log').write_text(
                'test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n'
                'test result: FAILED. 1 passed; 2 failed; 1 ignored; 0 measured; 4 filtered out;\n')
            with patch.dict(os.environ, {'OUTCOME_WORKSPACE': 'failure'}, clear=True):
                result = compact_results.summary(root, 'abc123')
            self.assertEqual(result['commit'], 'abc123')
            self.assertEqual(result['steps']['workspace'], 'failure')
            self.assertEqual(result['logs']['workspace']['test_summaries'], 2)
            self.assertEqual(result['logs']['workspace']['totals'],
                             {'passed': 4, 'failed': 2, 'ignored': 1, 'measured': 0, 'filtered': 4})
            self.assertEqual(len(result['logs']['workspace']['sha256']), 64)
            self.assertNotIn('reference', result['logs'])


if __name__ == '__main__':
    unittest.main()

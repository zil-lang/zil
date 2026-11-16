"""Require observed rustc execution rather than infer it from a request label."""
import json
from pathlib import Path
import tempfile
import unittest

from large_project_matrix import rust_compiler_scope
import rust_compare as rc


class ScopeTests(unittest.TestCase):
    def test_fresh_and_invoked_artifacts_are_distinguished(self):
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root)
            for fresh, stderr in [(True, 'Fresh crate\nFinished dev\n'),
                                  (False, 'Dirty crate\nRunning `/tool/rustc --crate-name crate`\n')]:
                (directory/'stdout.bin').write_text(json.dumps({'reason': 'compiler-artifact',
                    'target': {'name': rc.PACKAGE}, 'fresh': fresh})+'\n')
                (directory/'stderr.bin').write_text(stderr)
                observed = rust_compiler_scope(directory)
                self.assertEqual(observed['crate_artifacts_fresh'], [fresh])
                self.assertEqual(observed['rustc_invoked'], not fresh)


if __name__ == '__main__':
    unittest.main()

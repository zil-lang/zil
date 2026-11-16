"""Real SDK identity checks remain mandatory when payload recording is disabled."""
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent))
import native_link_matched_measure as matched


class NativeProfileRecordingTests(unittest.TestCase):
    def fixture(self, root):
        sdk = root / 'profile/root'
        (sdk / 'lib').mkdir(parents=True)
        payload = sdk / 'lib/input'
        payload.write_bytes(b'actual native SDK input\0' * 64)
        payload.chmod(0o644)
        data = ('DIR 2f\nDIR 2f6c6962\nFILE '
                + '/lib/input'.encode().hex() + ' ' + str(payload.stat().st_size)
                + ' 644 ' + matched.file_hash(payload) + '\n').encode('ascii')
        manifest = root / 'profile/manifest'
        manifest.write_bytes(data)
        out = root / 'out'
        out.mkdir()
        args = types.SimpleNamespace(native_link_profile=manifest,
            native_link_profile_sha256=matched.file_hash(manifest),
            candidate_source='f' * 40, repo=root, out=out)
        return args, payload, data

    def test_default_audit_does_not_copy_sdk_but_still_rejects_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            args, payload, policy = self.fixture(Path(directory))
            with patch.object(matched.subprocess, 'check_output', return_value=policy):
                result = matched.archive_native_profile(args)
                self.assertFalse(result['archived_inputs'])
                self.assertEqual(result['files'][0]['sha256'], matched.file_hash(payload))
                self.assertNotIn('archive', result['files'][0])
                self.assertEqual(list(args.out.rglob('*.sdk-input')), [])
                payload.write_bytes(b'corrupt')
                with self.assertRaisesRegex(ValueError, 'SDK input'):
                    matched.archive_native_profile(args)

    def test_explicit_archive_preserves_actual_sdk_bytes_and_unknown_files_reject(self):
        with tempfile.TemporaryDirectory() as directory:
            args, payload, policy = self.fixture(Path(directory))
            with patch.object(matched.subprocess, 'check_output', return_value=policy):
                result = matched.archive_native_profile(args, archive_inputs=True)
                self.assertTrue(result['archived_inputs'])
                self.assertEqual((args.out / result['files'][0]['archive']).read_bytes(), payload.read_bytes())
                payload.with_name('unlisted').write_bytes(b'extra')
                with self.assertRaisesRegex(ValueError, 'extra or unknown'):
                    matched.archive_native_profile(args)


if __name__ == '__main__':
    unittest.main()

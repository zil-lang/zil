#!/usr/bin/env python3
"""Archive/qualifier adversarial checks; synthetic bytes are never executed."""
import copy
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

DIRECTORY = Path(__file__).resolve().parent
NATIVE = DIRECTORY.parent / 'reactive-query-graph'
sys.path.insert(0, str(NATIVE))


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


runner = load('archive_native_measure', NATIVE / 'native_measure.py')
validator = load('archive_profile_validator', DIRECTORY / 'validate_push_profile.py')
ELF = b'\x7fELF' + bytes(range(60)) + b'complete-payload-and-trailing-bytes'


class ExecutableArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.run = self.root / 'run'
        self.run.mkdir()
        self.worker = self.run / 'block-0-baseline'
        self.worker.mkdir()
        self.program = self.worker / 'program'
        self.program.write_bytes(ELF)

    def archive(self):
        return runner.archive_executable(self.program, self.run, self.worker)

    def row(self, item):
        return {'executable_sha256': item['sha256'], 'executable_archive': item}

    def test_complete_single_read_both_arms_deduplicate_and_survive_overwrite(self):
        original_read = Path.read_bytes
        reads = []

        def counted_read(path):
            reads.append(path)
            return original_read(path)

        with patch.object(Path, 'read_bytes', counted_read):
            first = self.archive()
        self.assertEqual(reads.count(self.program), 1)
        self.assertEqual(first, {'path': 'executable-blobs/' + validator.sha(ELF) + '.elf',
                                 'sha256': validator.sha(ELF), 'bytes': len(ELF)})
        candidate = self.run / 'block-0-candidate'
        candidate.mkdir()
        (candidate / 'program').write_bytes(ELF)
        self.assertEqual(first, runner.archive_executable(candidate / 'program', self.run, candidate))
        self.assertEqual(first, self.archive())
        self.assertEqual(len(list((self.run / 'executable-blobs').iterdir())), 1)
        second_elf = ELF + b'new-parent-or-recovery'
        self.program.write_bytes(second_elf)
        second = self.archive()
        self.assertNotEqual(first['sha256'], second['sha256'])
        self.program.unlink()  # Historical validation must never read worker/program.
        self.assertEqual(validator.check_executable_archive(self.run, self.row(first)), ELF)
        self.assertEqual(validator.check_executable_archive(self.run, self.row(second)), second_elf)

    def test_existing_blob_magic_length_or_payload_tampering_is_rejected_without_repair(self):
        first = self.archive()
        path = self.run / first['path']
        for bad in (b'NOPE' + ELF[4:], ELF[:-1], ELF[:-1] + b'!', ELF + b'extra'):
            path.write_bytes(bad)
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.archive()
            self.assertEqual(path.read_bytes(), bad)

    def test_source_is_not_followed_through_symlinks_or_external_paths(self):
        outside = self.root / 'outside'
        outside.mkdir()
        (outside / 'program').write_bytes(ELF)
        self.program.unlink()
        self.program.symlink_to(outside / 'program')
        with self.assertRaises(ValueError):
            self.archive()
        with self.assertRaises(ValueError):
            runner.archive_executable(outside / 'program', self.run, outside)
        self.program.unlink()
        self.worker.rmdir()
        self.worker.symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.archive()

    def test_archive_file_and_directory_symlinks_are_rejected_even_inside_run(self):
        first = self.archive()
        path = self.run / first['path']
        path.unlink()
        path.symlink_to(self.program)
        with self.assertRaises(ValueError):
            self.archive()
        with self.assertRaises(ValueError):
            validator.check_executable_archive(self.run, self.row(first))
        path.unlink()
        path.parent.rmdir()
        inside = self.run / 'inside'
        inside.mkdir()
        (inside / path.name).write_bytes(ELF)
        path.parent.symlink_to(inside, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.archive()
        with self.assertRaises(ValueError):
            validator.check_executable_archive(self.run, self.row(first))

    def test_non_elf_and_truncated_header_are_rejected(self):
        for bad in (b'', b'\x7fELF', b'x' * 100, b'\x7fELF' + b'x' * 59):
            self.program.write_bytes(bad)
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.archive()

    def test_validator_rehashes_and_rejects_missing_or_tampered_complete_bytes(self):
        first = self.archive()
        path = self.run / first['path']
        for bad in (b'NOPE' + ELF[4:], ELF[:-1], ELF[:-1] + b'!', ELF + b'extra'):
            path.write_bytes(bad)
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                validator.check_executable_archive(self.run, self.row(first))
        path.unlink()
        with self.assertRaises(ValueError):
            validator.check_executable_archive(self.run, self.row(first))
        with self.assertRaises(ValueError):
            validator.check_executable_archive(self.run, {'executable_sha256': first['sha256']})

    def test_validator_rejects_metadata_and_path_escape_or_row_hash_mismatch(self):
        first = self.archive()
        for key, value in [('path', '/tmp/external.elf'), ('path', '../external.elf'),
                           ('path', 'executable-blobs/../other.elf'),
                           ('sha256', 'f' * 64), ('bytes', len(ELF) - 1), ('bytes', True)]:
            wrong = dict(first, **{key: value})
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                validator.check_executable_archive(self.run, {'executable_sha256': first['sha256'],
                                                              'executable_archive': wrong})
        with self.assertRaises(ValueError):
            validator.check_executable_archive(self.run, {'executable_sha256': 'f' * 64,
                                                          'executable_archive': first})


class IndependentToolIdentityTests(unittest.TestCase):
    def manifest(self):
        return {'schema': 'native-measure-v2', 'tool_commit': 'e' * 40,
                'compiled_source': {'baseline': validator.BASELINE, 'candidate': validator.ELF_ARCHIVE_SOURCE},
                'executable_archiving_policy': copy.deepcopy(validator.EXECUTABLE_ARCHIVING_POLICY)}

    def test_new_source_requires_explicit_tool_commit_and_complete_archive_policy(self):
        manifest = self.manifest()
        self.assertEqual(runner.EXECUTABLE_ARCHIVING_POLICY, validator.EXECUTABLE_ARCHIVING_POLICY)
        self.assertEqual(validator.check_tool_identity(manifest, validator.ELF_ARCHIVE_SOURCE, 'e' * 40), 'e' * 40)
        for wrong in (None, 'HEAD', 'e' * 7, 'f' * 40):
            with self.subTest(wrong=wrong), self.assertRaises(ValueError):
                validator.check_tool_identity(manifest, validator.ELF_ARCHIVE_SOURCE, wrong)
        for key, value in [('schema', 'native-measure-v1'), ('tool_commit', validator.ELF_ARCHIVE_SOURCE),
                           ('compiled_source', {'candidate': 'e' * 40}),
                           ('executable_archiving_policy', None)]:
            bad = dict(manifest, **{key: value})
            with self.subTest(key=key), self.assertRaises(ValueError):
                validator.check_tool_identity(bad, validator.ELF_ARCHIVE_SOURCE, 'e' * 40)
        bad = self.manifest()
        del bad['executable_archiving_policy']['scope']
        with self.assertRaises(ValueError):
            validator.check_tool_identity(bad, validator.ELF_ARCHIVE_SOURCE, 'e' * 40)

    def test_historical_profiles_cannot_be_relabelled_with_new_tools(self):
        for source in validator.CANDIDATES:
            if source == validator.ELF_ARCHIVE_SOURCE:
                continue
            self.assertEqual(validator.check_tool_identity({}, source, None), source)
            with self.assertRaises(ValueError):
                validator.check_tool_identity(self.manifest(), source, 'e' * 40)


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env python3
"""Pure synthetic installation/adversarial tests; no compiler, linker or bwrap runs."""
import importlib.util
from pathlib import Path
import stat
import struct
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('native_link_profile', Path(__file__).with_name('build_native_link_profile.py'))
profile = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = profile
SPEC.loader.exec_module(profile)


def elf_bytes(kind=3, interpreter=None, needed=(), soname=None, runpath=None, text_name='.text'):
    dynamic_strings = b'\0'
    dynamic = []
    for tag, value in [(1, value) for value in needed] + ([(14, soname)] if soname else []) + ([(29, runpath)] if runpath is not None else []):
        dynamic.append((tag, len(dynamic_strings)))
        dynamic_strings += value.encode() + b'\0'
    specs = [('.shstrtab', 3, b'', 0, 0), (text_name, 1, b'\x90', 0, 0)]
    if dynamic:
        specs += [('.dynstr', 3, dynamic_strings, 0, 0),
                  ('.dynamic', 6, b''.join(struct.pack('<qQ', *pair) for pair in dynamic + [(0, 0)]), 3, 16)]
    names = b'\0'
    name_offsets = {}
    for name, *_ in specs:
        name_offsets[name] = len(names)
        names += name.encode() + b'\0'
    specs[0] = ('.shstrtab', 3, names, 0, 0)
    phnum = int(interpreter is not None)
    raw_interpreter = interpreter.encode()+b'\0' if interpreter else b''
    offset = 64 + 56 * phnum
    body = raw_interpreter
    offset += len(raw_interpreter)
    sections = [bytes(64)]
    for name, kind_id, data, link, entry_size in specs:
        sections.append(struct.pack('<IIQQQQIIQQ', name_offsets[name], kind_id, 0, 0,
                                    offset, len(data), link, 0, 1, entry_size))
        body += data
        offset += len(data)
    header = bytearray(64)
    header[:7] = b'\x7fELF\x02\x01\x01'
    struct.pack_into('<HHI', header, 16, kind, 62, 1)
    struct.pack_into('<QQ', header, 32, 64 if phnum else 0, offset)
    struct.pack_into('<HHHHHH', header, 52, 64, 56 if phnum else 0, phnum, 64, len(sections), 1)
    program = struct.pack('<IIQQQQQQ', 3, 0, 120, 0, 0, len(raw_interpreter), len(raw_interpreter), 1) if phnum else b''
    return bytes(header) + program + body + b''.join(sections)


def archive_bytes(payload):
    header = b'member.o/       ' + b'0           ' + b'0     ' + b'0     ' + b'100644  ' + str(len(payload)).encode().ljust(10) + b'`\n'
    assert len(header) == 60
    return b'!<arch>\n' + header + payload + (b'\n' if len(payload) & 1 else b'')


class FakeSDK:
    def __init__(self, root):
        self.root = root
        self.lib = root/'lib'
        self.lib.mkdir(parents=True)
        self.loader = self.lib/'ld-linux-x86-64.so.2'
        self.libc = self.lib/'libc.so.6'
        self.ld = root/'ld'
        self.ld_real = root/'ld-real'
        self.bwrap = root/'bwrap'
        self.clang = root/'clang'
        self.placeholder = root/'placeholder.o'
        self.loader.write_bytes(elf_bytes())
        self.libc.write_bytes(elf_bytes(needed=(self.loader.name,), soname='libc.so.6'))
        self.ld_real.write_bytes(elf_bytes(interpreter=str(self.loader), needed=('libc.so.6',)))
        self.ld.symlink_to(self.ld_real)
        self.bwrap.write_bytes(elf_bytes(interpreter=str(self.loader), needed=('libc.so.6',)))
        self.clang.write_bytes(b'trusted discovery-only fake clang')
        self.placeholder.write_bytes(elf_bytes(kind=1))
        self.nonshared = self.lib/'libc_nonshared.a'
        self.nonshared.write_bytes(archive_bytes(elf_bytes(kind=1)))
        (self.lib/'libpthread.a').write_bytes(archive_bytes(elf_bytes(kind=1)))
        (self.lib/'libc.so').write_text('/* finite installed script */\nOUTPUT_FORMAT(elf64-x86-64)\nGROUP('+str(self.libc)+' AS_NEEDED('+str(self.nonshared)+'))\n')
        for path in (self.ld_real, self.bwrap, self.loader, self.clang):
            path.chmod(0o755)
        self.calls = []
        self.bad_option = None
        self.probe_unsupported = False

    def commands(self, raw):
        argv = list(map(str, raw))
        self.calls.append(argv)
        if argv == [str(self.ld), '--verbose']:
            return 'GNU ld (synthetic SDK)\nSEARCH_DIR("='+str(self.lib)+'");\n', ''
        if argv[0] == str(self.clang):
            output = argv[argv.index('-o')+1]
            job = [str(self.ld), '-pie', '--hash-style=gnu', '-m', 'elf_x86_64', '-dynamic-linker',
                   str(self.loader), '-L'+str(self.lib), '-o', output, str(self.placeholder), '-lc']
            if '-pthread' in argv:
                job.append('-lpthread')
            if self.bad_option:
                job.append(self.bad_option)
            return '', 'clang synthetic installation\n '+ ' '.join('"'+word+'"' for word in job)+'\n'
        if argv[0] == '/usr/bin/ldd':
            if argv[1] == str(self.libc):
                return str(self.loader)+' (0x123)\n', ''
            return 'libc.so.6 => '+str(self.libc)+' (0x123)\n'+str(self.loader)+' (0x456)\n', ''
        if '--unshare-user' in argv:
            if self.probe_unsupported:
                raise profile.Unsupported('synthetic read-only uid_map / namespace unavailable')
            return 'GNU ld (synthetic isolated snapshot)\n', ''
        raise AssertionError('unexpected real-tool request: '+repr(argv))

    def install(self, snapshot, policy):
        return profile.install(clang=self.clang, ld=self.ld, bwrap=self.bwrap,
                               placeholder=self.placeholder, snapshot_root=snapshot,
                               policy_output=policy, commands=self.commands)


class ProfileInstallationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.sdk = FakeSDK(self.root/'sdk')
        self.snapshot = self.root/'profile.root'
        self.policy = self.root/'profile.policy'

    def test_complete_four_variant_materialization_and_canonical_policy(self):
        result = self.sdk.install(self.snapshot, self.policy)
        data = self.policy.read_bytes()
        self.assertEqual(result['profile_id'], profile.digest(data))
        self.assertEqual(result['variants'], 4)
        lines = data.decode().splitlines()
        self.assertEqual(lines[:2], ['ZILLINK1', 'TARGET '+profile.hextext(profile.TARGET)])
        self.assertEqual([line for line in lines if line.startswith('VARIANT ')],
                         ['VARIANT O0 0', 'VARIANT O0 1', 'VARIANT O2 0', 'VARIANT O2 1'])
        self.assertEqual((lines.count('OBJECTS'), lines.count('OUTPUT'), lines.count('ENDVARIANT')), (4, 4, 4))
        self.assertEqual(lines[-1], 'END')
        self.assertNotIn(str(self.snapshot).encode().hex(), data.decode())
        self.assertNotIn(profile.hextext(self.sdk.placeholder), data.decode())
        for alias in (self.sdk.ld, self.sdk.ld_real):
            path = self.snapshot/str(alias).lstrip('/')
            self.assertTrue(path.is_file())
            self.assertFalse(path.is_symlink())
            self.assertEqual(path.read_bytes(), self.sdk.ld_real.read_bytes())
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o755)
        self.assertFalse(any(path.is_symlink() for path in self.snapshot.rglob('*')))
        self.assertTrue((self.snapshot/'inputs').is_dir())
        self.assertEqual(list((self.snapshot/'out').iterdir()), [])
        job_calls = [call for call in self.sdk.calls if call[0] == str(self.sdk.clang)]
        self.assertEqual([(call[3], '-pthread' in call) for call in job_calls],
                         [('-O0', False), ('-O0', True), ('-O2', False), ('-O2', True)])
        probe = self.sdk.calls[-1]
        for flag in ('--unshare-user', '--unshare-pid', '--unshare-net', '--die-with-parent', '--clearenv'):
            self.assertIn(flag, probe)
        self.assertEqual(probe.count('--inhibit-cache'), 2)
        self.assertEqual(probe.count('--glibc-hwcaps-mask'), 2)
        self.assertEqual(probe[-1], '--version')

    def test_snapshot_external_root_does_not_change_manifest_identity(self):
        first = self.sdk.install(self.snapshot, self.policy)
        second_policy = self.root/'second.policy'
        second = self.sdk.install(self.root/'different-snapshot.root', second_policy)
        self.assertEqual(self.policy.read_bytes(), second_policy.read_bytes())
        self.assertEqual(first['profile_id'], second['profile_id'])

    def test_namespace_failure_never_publishes_profile_or_policy(self):
        self.sdk.probe_unsupported = True
        with self.assertRaisesRegex(profile.Unsupported, 'uid_map'):
            self.sdk.install(self.snapshot, self.policy)
        self.assertFalse(self.snapshot.exists())
        self.assertFalse(self.policy.exists())
        self.assertEqual(list(self.root.glob('.zil-native-link-stage-*')), [])

    def test_unknown_driver_inputs_decline_without_partial_profile(self):
        for option in ('-plugin', '@response', '--sysroot=/host', '-flto', '-z', '--unknown'):
            self.sdk.bad_option = option
            with self.subTest(option=option), self.assertRaises(profile.Unsupported):
                self.sdk.install(self.snapshot, self.policy)
            self.assertFalse(self.snapshot.exists())
            self.assertFalse(self.policy.exists())

    def test_existing_output_is_never_replaced(self):
        self.policy.write_bytes(b'unique existing policy')
        with self.assertRaises(profile.Unsupported):
            self.sdk.install(self.snapshot, self.policy)
        self.assertEqual(self.policy.read_bytes(), b'unique existing policy')

    def test_materialization_rejects_changed_install_and_copies_alias_bytes(self):
        inventory = profile.Inventory(self.sdk.commands)
        inventory.file(self.sdk.ld)
        self.sdk.ld_real.write_bytes(self.sdk.ld_real.read_bytes()+b'changed')
        with self.assertRaisesRegex(profile.Unsupported, 'changed'):
            profile.materialize(inventory, self.snapshot)

    def test_script_recursive_cycle_is_rejected(self):
        (self.sdk.lib/'libc.so').write_text('GROUP('+str(self.sdk.lib/'libc.so')+')')
        with self.assertRaisesRegex(profile.Unsupported, 'cycle'):
            self.sdk.install(self.snapshot, self.policy)

    def test_library_closure_includes_all_search_candidates(self):
        other = self.root/'other'
        other.mkdir()
        alternate = other/'libpthread.a'
        alternate.write_bytes(archive_bytes(elf_bytes(kind=1)))
        inventory = profile.Inventory(self.sdk.commands)
        inventory.library('pthread', [self.sdk.lib, other])
        self.assertIn(str(alternate), inventory.files)
        self.assertIn(str(self.sdk.lib/'libpthread.a'), inventory.files)

    def test_host_preload_is_a_required_absence_predicate(self):
        from unittest.mock import patch
        original = Path.exists
        with patch.object(Path, 'exists', lambda path: True if path == Path('/etc/ld.so.preload') else original(path)):
            with self.assertRaisesRegex(profile.Unsupported, 'preload'):
                self.sdk.install(self.snapshot, self.policy)

    def test_symlink_ancestor_is_resolved_before_parent_components(self):
        actual = self.root/'actual'
        (actual/'child').mkdir(parents=True)
        selected = actual/'tool'
        selected.write_bytes(elf_bytes())
        (self.root/'alias').symlink_to(actual/'child', target_is_directory=True)
        lexical = self.root/'tool'
        lexical.write_bytes(b'a different unselected host file')
        inventory = profile.Inventory(self.sdk.commands)
        canonical, data = inventory.file(self.root/'alias/../tool')
        self.assertEqual(canonical, selected)
        self.assertEqual(data, selected.read_bytes())
        self.assertNotIn(str(lexical), inventory.files)

    def test_runtime_same_soname_different_payload_is_unsupported(self):
        first, second = self.root/'first.so', self.root/'second.so'
        first.write_bytes(elf_bytes(soname='same.so.1'))
        second.write_bytes(elf_bytes(soname='same.so.1', text_name='.different'))
        inventory = profile.Inventory(self.sdk.commands)
        inventory.runtime(first)
        with self.assertRaisesRegex(profile.Unsupported, 'SONAME'):
            inventory.runtime(second)

    def test_marker_cardinality_and_interpreter_cannot_be_overridden(self):
        for extra in (str(self.sdk.placeholder), '-o', '-dynamic-linker'):
            self.sdk.bad_option = extra
            with self.subTest(extra=extra), self.assertRaises(profile.Unsupported):
                self.sdk.install(self.snapshot, self.policy)


class ClosedFormatTests(unittest.TestCase):
    def test_driver_requires_exactly_one_closed_absolute_job(self):
        self.assertEqual(profile.driver_job('metadata\n "/usr/bin/ld" "-o" "/out"\n'), ['/usr/bin/ld', '-o', '/out'])
        for text in ('no job', '"/usr/bin/ld"\n"/usr/bin/as"', '"ld" "-o" "/out"', '"/usr/bin/ld', '"/usr/bin/ld" ""'):
            with self.subTest(text=text), self.assertRaises(profile.Unsupported):
                profile.driver_job(text)

    def test_unknown_or_unbalanced_linker_scripts_are_rejected(self):
        expected = ['/lib/a.so', '-lc', 'libb.a']
        self.assertEqual(profile.script_members(b'/* x */OUTPUT_FORMAT(elf64-x86-64) GROUP(/lib/a.so AS_NEEDED(-lc libb.a))'), expected)
        for text in (b'INCLUDE(/host/file)', b'SEARCH_DIR(/host)', b'INPUT(relative.o)', b'GROUP(/a',
                     b'OUTPUT_FORMAT(elf32-i386) INPUT(/a)', b'INPUT(/a) trailing', b'/* unterminated',
                     b'GROUP()', b'GROUP(/a AS_NEEDED())', b'INPUT(/a/../b)'):
            with self.subTest(text=text), self.assertRaises(profile.Unsupported):
                profile.script_members(text)

    def test_default_search_roots_and_ldd_must_be_complete(self):
        self.assertEqual(profile.search_directories('GNU ld\nSEARCH_DIR("=/lib"); SEARCH_DIR("/usr/lib");'),
                         [Path('/lib'), Path('/usr/lib')])
        for text in ('SEARCH_DIR("/lib")', 'GNU ld\nSEARCH_DIR("relative")', 'GNU ld\nSEARCH_DIR("/lib"); SEARCH_DIR(bad)'):
            with self.assertRaises(profile.Unsupported):
                profile.search_directories(text)
        self.assertEqual(profile.dependency_rows('linux-vdso.so.1 (0)\nlibc.so.6 => /lib/libc.so.6 (1)\n/lib/ld.so (2)'),
                         [Path('/lib/libc.so.6'), Path('/lib/ld.so')])
        for text in ('libmissing.so => not found', 'statically linked', '', 'libc.so.6 => relative'):
            with self.assertRaises(profile.Unsupported):
                profile.dependency_rows(text)

    def test_complete_native_archives_and_elf_only(self):
        data = elf_bytes(kind=1)
        profile.native_archive(archive_bytes(data))
        profile.native_archive(b'!<arch>\n')  # Modern glibc's libpthread.a/libdl.a.
        self.assertEqual(profile.elf(data).kind, 1)
        for bad in (b'!<thin>\n', archive_bytes(b'BC\xc0\xde'), archive_bytes(data)[:-1], archive_bytes(elf_bytes(kind=3))):
            with self.subTest(prefix=bad[:20]), self.assertRaises(profile.Unsupported):
                profile.native_archive(bad)
        for bad in (data[:-1], data.replace(b'\x7fELF', b'NOPE', 1), elf_bytes(kind=1, text_name='.gnu.lto_body'), elf_bytes(runpath='$ORIGIN')):
            with self.subTest(prefix=bad[:20]), self.assertRaises(profile.Unsupported):
                profile.elf(bad)
        unknown = bytearray(data)
        table = struct.unpack_from('<Q', unknown, 40)[0]
        struct.pack_into('<I', unknown, table + 2 * 64 + 4, 0x60000000)
        with self.assertRaisesRegex(profile.Unsupported, 'section type'):
            profile.elf(unknown)

    def test_explicit_library_path_cannot_silently_fall_back_to_host(self):
        inventory = profile.Inventory(lambda argv: ('', ''))
        inventory.needed_hashes['libabsent.so.1'] = 'a'*64
        inventory.library_dirs.add('/closed/lib')
        with self.assertRaisesRegex(profile.Unsupported, 'cannot reproduce'):
            inventory.verify_loader_lookup()


if __name__ == '__main__':
    unittest.main()

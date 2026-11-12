import unittest

from analyze_trace import analyze, stitch


class TraceTests(unittest.TestCase):
    def test_interleaved_failures_are_retained(self):
        text = ('1 openat(AT_FDCWD, "/missing", O_RDONLY <unfinished ...>\n'
                '2 access("/present", F_OK) = 0\n'
                '1 <... openat resumed>) = -1 ENOENT (No such file or directory)\n')
        result = analyze(text)
        self.assertEqual(result['errors'], {'ENOENT': 1})
        self.assertEqual(result['paths']['/missing']['results'], {'failure:ENOENT': 1})
        self.assertFalse(result['closure_proven'])

    def test_directory_fd_and_exec_argv_do_not_become_read_set(self):
        text = ('1 execve("/tool", ["/tool", "/output"], 0x123 /* 2 vars */) = 0\n'
                '1 openat(AT_FDCWD, "/dir", O_RDONLY|O_DIRECTORY) = 3\n'
                '1 getdents64(3, 0x123 /* 3 entries */, 32768) = 72\n'
                '1 read(0x3, 0x456, 0x10) = 0x10\n'
                '1 close(3) = 0\n')
        result = analyze(text)
        self.assertEqual(result['enumerations'][0]['directory'], '/dir')
        self.assertFalse(result['enumerations'][0]['names_recorded'])
        self.assertNotIn('/output', result['paths'])
        self.assertIn('read', result['raw_argument_syscalls'])

    def test_truncation_or_unmatched_resume_is_rejected(self):
        with self.assertRaises(ValueError):
            list(stitch('1 openat(AT_FDCWD, "/x", O_RDONLY <unfinished ...>\n'))
        with self.assertRaises((ValueError, KeyError)):
            list(stitch('1 <... openat resumed>) = 3\n'))


if __name__ == '__main__':
    unittest.main()

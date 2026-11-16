"""The short direct-edit history has no post-edit no-op or restore request."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).parent))
from restart_edit_matrix import NAMES, summarize_disks


class DirectEditDiskTests(unittest.TestCase):
    def test_short_history_preserves_boundaries_without_long_history_labels(self):
        def snapshot(value):
            return {'categories': {'candidate.semantic': {'physical_bytes': value}}}
        rows = []
        for block in [0, 1]:
            before = 0
            for request, after in zip(NAMES, [10, 10, 13, 13, 15]):
                rows.append({'preset': '5105', 'block': block, 'request': request,
                    'disk_before': snapshot(before), 'disk_after': snapshot(after)})
                before = after
        result = summarize_disks(rows, ['physical_bytes'])
        self.assertEqual(len(result), 2)
        for record in result:
            category = record['categories']['candidate.semantic']
            self.assertEqual(category['observed_boundary_peak']['physical_bytes'], 15)
            self.assertEqual(category['growth_from_empty']['physical_bytes'], 15)
            self.assertEqual(category['growth_from_cold']['physical_bytes'], 5)
            self.assertEqual(category['direct_restart_edit_growth']['physical_bytes'], 3)
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            summarize_disks([row for row in rows if row['request'] != 'private-001'], ['physical_bytes'])


if __name__ == '__main__':
    unittest.main()

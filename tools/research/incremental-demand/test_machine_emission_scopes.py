#!/usr/bin/env python3
"""Reject malformed MC observations and retain mixed-CGU unknowns end to end."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import push_counters as pc


def native(mc=None):
    stats = dict.fromkeys(pc.NATIVE_PRODUCT_FIELDS + pc.NATIVE_PROJECTION_FIELDS
                          + pc.NATIVE_IR_SCOPE_FIELDS, 0)
    stats.update(available=1, observer_schema=3, machine_emission=mc)
    return stats


def emission():
    mc = dict.fromkeys(pc.MACHINE_EMISSION_FIELDS, 0)
    mc.update(observer_schema=1, available=1, replayed_functions=2,
              replayed_blocks=3, machine_instructions_skipped=9,
              mc_replay_events=5, current_cfi_events=1, failed_replays=1,
              capture_ns=11, preflight_ns=13, replay_ns=17)
    return mc


def report(*stats):
    return {'native_codegen': {'object_reports': [{'native_products': s} for s in stats]}}


class MachineEmissionScopes(unittest.TestCase):
    def test_legacy_absent_null_and_unavailable_are_unknown(self):
        empty = dict.fromkeys(pc.MACHINE_EMISSION_FIELDS, 0)
        empty['observer_schema'] = 1
        old = native()
        del old['machine_emission']
        for stats in (None, old, native(), native(empty)):
            with self.subTest(stats=stats):
                unit = pc.inspect_native_products(report(stats))['units'][0]
                self.assertIsNone(unit['machine_emission'])
                self.assertIs(unit['stats'], stats)

    def test_malformed_counters_do_not_become_zero(self):
        good = emission()
        for field in pc.MACHINE_EMISSION_FIELDS:
            for value in (None, -1, True, 1.0, '1'):
                bad = dict(good, **{field: value})
                with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                    pc.inspect_native_products(report(native(bad)))
        for schema in (None, True, 0, 2, '1'):
            with self.subTest(schema=schema), self.assertRaises(ValueError):
                pc.inspect_native_products(report(native(dict(good, observer_schema=schema))))
        for bad in ([], 'unsupported', dict(good, available=2), dict(good, available=0)):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                pc.inspect_native_products(report(native(bad)))
        del good['replayed_functions']
        with self.assertRaises(ValueError):
            pc.inspect_native_products(report(native(good)))

    def test_mixed_current_legacy_and_fallback_scopes_survive_cli(self):
        mc = emission()
        stats = native(mc)
        stats['fresh_context_fallbacks'] = 1
        original = copy.deepcopy(stats)
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp)
            manifest = {'source_shas': {'candidate': 'f' * 40}, 'binaries': {}, 'arguments': {}}
            (run / 'manifest.json').write_text(json.dumps(manifest))
            row = {'valid': True, 'arm': 'candidate', 'block': 0, 'request': 'private',
                   'measured': True, 'transition_from': 'cold', 'measurement': {'wall_ms': 1},
                   'resident': {'report': report(stats, native(), None)}}
            rust = {**row, 'arm': 'rust', 'resident': None}
            (run / 'rows.jsonl').write_text('\n'.join(map(json.dumps, (row, rust))) + '\n')
            output = run / 'summary.json'
            result = subprocess.run([sys.executable, str(Path(__file__).with_name('native_scope_summary.py')),
                                     str(run), '--output', str(output)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads(output.read_text())
            current, rust = summary['rows']
            self.assertEqual(current['machine_emission_observed_units'], 1)
            self.assertEqual(current['machine_emission_unobserved_units'], 2)
            totals = current['machine_emission_observed_totals']
            self.assertEqual(totals['machine_instructions_skipped'], 9)
            self.assertEqual(totals['mc_replay_events'], 5)
            self.assertEqual(totals['current_cfi_events'], 1)
            self.assertEqual(totals['failed_replays'], 1)
            self.assertNotIn('machine_emission', current['native_observed_totals'])
            self.assertIsNone(rust['machine_emission_observed_totals'])
            self.assertIn('discarded replay is not final work avoided', summary['scope'])
        self.assertEqual(stats, original)


if __name__ == '__main__':
    unittest.main()

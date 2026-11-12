#!/usr/bin/env python3
"""No compiler subprocesses: evidence/session/diagnostic regression checks."""
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import matched_measure as mm
import rust_compare as rc


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.out = self.root/'out'
        self.out.mkdir()
        self.args = SimpleNamespace(out=self.out, mode='o2', nm='/usr/bin/nm')
        self.target = self.root/'target'
        self.incremental = self.target/'release'/'incremental'
        self.symbol_patch = patch.object(mm, 'symbols', return_value={'path':'mock','sha256':'mock'})
        self.symbol_patch.start()
        self.addCleanup(self.symbol_patch.stop)

    def session(self, name, body=b'object'):
        directory = self.incremental/(rc.PACKAGE+'-hash')/name
        directory.mkdir(parents=True)
        (directory/'work-products.bin').write_bytes(b'work-products')
        (directory/'a.o').write_bytes(body)
        return directory

    def test_new_session_selects_current_while_old_remains(self):
        old = self.session('s-old')
        first = mm.rust_inventory(self.args,self.target,{})
        new = self.session('s-new',b'changed object')
        second = mm.rust_inventory(self.args,self.target,first)
        self.assertEqual(second['current_session'],str(new.relative_to(self.incremental)))
        self.assertEqual(second['object_count'],1)
        self.assertEqual(len(second['historical_objects']),1)
        self.assertEqual(second['historical_objects'][0]['path'],str((old/'a.o').relative_to(self.incremental)))

    def test_unchanged_session_preserves_identity(self):
        self.session('s-one')
        first = mm.rust_inventory(self.args,self.target,{})
        second = mm.rust_inventory(self.args,self.target,first)
        self.assertTrue(second['objects'][0]['same_inode_bytes_mtime_as_previous_request'])
        self.assertEqual(first['current_session'],second['current_session'])

    def test_equal_bytes_different_object_is_not_physical_reuse(self):
        self.session('s-one')
        first = mm.rust_inventory(self.args,self.target,{})
        self.session('s-two')
        second = mm.rust_inventory(self.args,self.target,first)
        self.assertTrue(second['objects'][0]['same_bytes_as_previous_request'])
        self.assertFalse(second['objects'][0]['same_inode_bytes_mtime_as_previous_request'])

    def test_ambiguous_sessions_fail_instead_of_guessing(self):
        self.session('s-one')
        self.session('s-two')
        with self.assertRaisesRegex(ValueError,'ambiguous'):
            mm.rust_inventory(self.args,self.target,{})

    def test_zil_uses_current_action_receipt_not_cache_size(self):
        cache=self.root/'cache'
        cache.mkdir()
        key='12'*32
        data=b'object payload'
        header=b'ZILOBJ01'+bytes.fromhex(key)+b'x'*32+len(data).to_bytes(8,'little')
        (cache/(key+'.zobj')).write_bytes(header+data)
        (cache/'old-unrelated.zobj').write_bytes(b'old object')
        report={'native_codegen':{'actual_units':1,'object_reports':[{'cache':{'action_key':key,'state':'hit'}}]}}
        result=mm.zil_inventory(self.args,report,cache)
        self.assertEqual(result['object_count'],1)
        self.assertEqual(result['objects'][0]['object']['sha256'],rc.sha(data))
        self.assertEqual((self.out/result['objects'][0]['object']['archive']).read_bytes(),data)

    def test_precise_zil_error_rejects_wrong_source_and_offset(self):
        source=self.root/'source'
        files={'m0.zil':'return absent_name'}
        report={'diagnostics':[{'code':'E-UNKNOWN-NAME','span':{'file':1,'start':7,'end':18}}],
                'source_files':{'1':str(source/'m0.zil')}}
        self.assertTrue(mm.zil_error(report,source,files))
        report['diagnostics'][0]['span']['start']=6
        self.assertFalse(mm.zil_error(report,source,files))
        report['diagnostics'][0]['span']['start']=7
        report['source_files']['1']=str(source/'m1.zil')
        self.assertFalse(mm.zil_error(report,source,files))

    def test_check_has_no_fabricated_codegen_inventory(self):
        self.args.mode='check'
        inventory=mm.rust_inventory(self.args,self.target,{})
        self.assertEqual(inventory['objects'],[])
        self.assertIsNone(inventory['current_session'])


if __name__=='__main__':
    unittest.main()

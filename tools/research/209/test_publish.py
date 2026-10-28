import hashlib,importlib.util,subprocess,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('publish',str(Path(__file__).with_name('publish.py')))
p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)
class Tests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();root=Path(self.tmp.name)
        self.origin=root/'origin.git';self.author=root/'author';self.reader=root/'reader'
        def cmd(*a):return subprocess.check_output(a,text=True,stderr=subprocess.DEVNULL).strip()
        cmd('git','init','--bare',str(self.origin));cmd('git','init','-b','main',str(self.author))
        p.git(self.author,'config','user.name','Test');p.git(self.author,'config','user.email','test@example.invalid')
        (self.author/'file').write_text('base');p.git(self.author,'add','file');p.git(self.author,'commit','-m','base')
        self.base=p.git(self.author,'rev-parse','HEAD');p.git(self.author,'remote','add','origin',str(self.origin));p.git(self.author,'push','origin','main')
        p.git(self.origin,'symbolic-ref','HEAD','refs/heads/main');cmd('git','clone',str(self.origin),str(self.reader))
        p.git(self.author,'switch','-c','integration/209-mainline');(self.author/'file').write_text('candidate');p.git(self.author,'commit','-am','candidate')
        self.head=p.git(self.author,'rev-parse','HEAD');self.bundle=root/'update.bundle'
        p.git(self.author,'bundle','create',str(self.bundle),'refs/heads/integration/209-mainline','^'+self.base)
        self.manifest={'remote_base':self.base,'candidate_commit':self.head,'bundle_sha256':hashlib.sha256(self.bundle.read_bytes()).hexdigest()}
    def tearDown(self):self.tmp.cleanup()
    def test_prepare_does_not_write_remote_or_change_checkout(self):
        with patch.object(p,'check_origin'):r=p.execute(self.reader,self.bundle,self.manifest)
        self.assertEqual(r['status'],'Prepared');self.assertEqual(p.git(self.origin,'rev-parse','main'),self.base)
        self.assertEqual((self.reader/'file').read_text(),'base')
    def test_fast_forward_push_and_confirm_preserve_checkout(self):
        with patch.object(p,'check_origin'):r=p.execute(self.reader,self.bundle,self.manifest,True)
        self.assertEqual(r['status'],'Published');self.assertEqual(p.git(self.origin,'rev-parse','main'),self.head)
        self.assertEqual((self.reader/'file').read_text(),'base')
    def test_main_advance_is_rejected(self):
        p.git(self.author,'push','origin','HEAD:main')
        with patch.object(p,'check_origin'),self.assertRaisesRegex(RuntimeError,'main moved'):p.execute(self.reader,self.bundle,self.manifest,True)
    def test_digest_mismatch_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError,'SHA256'):p.execute(self.reader,self.bundle,dict(self.manifest,bundle_sha256='0'*64),True)
    def test_wrong_origin_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError,'expected'):p.check_origin(self.reader)
    def test_wrong_candidate_identity_is_rejected(self):
        with patch.object(p,'check_origin'),self.assertRaisesRegex(RuntimeError,'candidate identity'):p.execute(self.reader,self.bundle,dict(self.manifest,candidate_commit='1'*40),True)
if __name__=='__main__':unittest.main()

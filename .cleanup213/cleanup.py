#!/usr/bin/env python3
"""Retire exact archived heads atomically. Production main is never updated."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import zipfile

GATES = ['format','workspace','reference','native_build','consumer208','clippy']
SHA = re.compile(r'[0-9a-f]{40}\Z')

class Refusal(Exception):
    pass

def require(ok, message):
    if not ok: raise Refusal(message)

def sha(data):
    return hashlib.sha256(data).hexdigest()

def redact(value):
    return re.sub(r'(https?://)[^/\s@]+@',r'\1[redacted]@',value)

def save(path, data):
    temporary=path.with_suffix(path.suffix+'.tmp')
    temporary.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
    temporary.replace(path)

class Cleanup:
    def __init__(self,args):
        self.args=args;self.repo=Path(args.repo).resolve();self.out=Path(args.out).resolve()
        self.out.mkdir(parents=True,exist_ok=True)
        require(not self.out.is_relative_to(self.repo),'receipts must be outside checkout')
        self.plan_bytes=Path(args.plan).read_bytes();self.p=json.loads(self.plan_bytes)
        p=self.p
        require(p['schema']==1 and p['repository']==args.repository,'repository/plan mismatch')
        require(p['remote']=='origin' and p['main_ref']=='refs/heads/main' and
                p['transport_ref']=='refs/heads/task/213','unexpected destination')
        for value in [args.transport_sha,p['expected_main'],p['expected_main_tree'],p['workflow_blob']]:
            require(SHA.fullmatch(value or ''),'invalid exact object identity')
        require(p['workflow_path']=='.github/workflows/verify-main.yml','unexpected source difference')
        require(len(p['retire'])==6 and len(p['canonical_archives'])==8,'unexpected retirement/archive set')
        self.expected={p['main_ref']:p['expected_main'],p['transport_ref']:args.transport_sha}
        for row in p['retire']:
            require(row['ref'].startswith('refs/heads/') and row['ref'] not in self.expected,'duplicate/protected retirement')
            require(SHA.fullmatch(row['expected_sha']) and row['disposition'] in ('merged','archive-only'),'invalid retirement policy')
            self.expected[row['ref']]=row['expected_sha']
        for tag,value in p['canonical_archives'].items():
            require(tag.startswith('refs/tags/archive/') and SHA.fullmatch(value),'invalid canonical archive')
        require(p['cleanup_tag']!=p['guard_tag'] and all(x.startswith('refs/tags/archive/') and
                x not in p['canonical_archives'] for x in [p['cleanup_tag'],p['guard_tag']]),'archive collision')
        for ref in [*self.expected,*p['canonical_archives'],p['cleanup_tag'],p['guard_tag']]:
            self.git('check-ref-format',ref)
        self.check_clean()

    def git(self,*args,data=None,env=None,allowed=(0,)):
        result=subprocess.run(['git',*args],cwd=self.repo,input=data,text=True,
                              stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
        require(result.returncode in allowed,'git '+args[0]+' failed: '+redact(result.stdout+'\n'+result.stderr))
        return result

    def check_clean(self):
        require(self.git('rev-parse','HEAD').stdout.strip()==self.args.transport_sha,'checkout is not triggering cleanup transport')
        require(not self.git('status','--porcelain=v1','--untracked-files=normal').stdout.strip(),'checkout changed')

    def remote(self):
        rows=self.git('ls-remote','--refs','origin','refs/heads/*','refs/tags/archive/*').stdout.splitlines()
        result={}
        for row in rows:
            value,ref=row.split('\t',1)
            require(SHA.fullmatch(value) and ref not in result,'invalid remote advertisement')
            result[ref]=value
        return result

    def snapshot(self):
        refs=self.remote()
        require({r:s for r,s in refs.items() if r.startswith('refs/heads/')}==self.expected,'unknown or drifted remote heads')
        require(all(refs.get(r)==s for r,s in self.p['canonical_archives'].items()),'canonical archive missing or drifted')
        require(self.p['cleanup_tag'] not in refs and self.p['guard_tag'] not in refs,'cleanup or guard already exists')
        return refs

    def proof(self):
        data=Path(self.args.proof).read_bytes()
        require(sha(data)==self.p['proof_sha256'],'proof ZIP checksum mismatch')
        allowed={'gates.json','manifest.json','consumer208.json',*[x+'.log' for x in GATES]}
        with zipfile.ZipFile(self.args.proof) as z:
            infos=z.infolist()
            require(len(infos)==len(allowed) and {x.filename for x in infos}==allowed,'unexpected proof entries')
            require(all(x.file_size<=5*1024*1024 for x in infos) and sum(x.file_size for x in infos)<=10*1024*1024,'proof size exceeded')
            files={name:z.read(name) for name in allowed}
        gates=json.loads(files['gates.json']);manifest=json.loads(files['manifest.json'])
        for key,value in self.p['proof_identity'].items():
            require(gates.get(key)==value,'proof identity mismatch: '+key)
        require(gates.get('status')=='Pass' and gates.get('stage')=='verified' and gates.get('finalized') is True,'six-gate proof is not finalized Pass')
        require(sha(files['manifest.json'])==gates['manifest_sha256'],'old manifest checksum mismatch')
        require(manifest['repository']==self.p['repository'] and manifest['source']['sha']==gates['source_sha'] and
                manifest['source']['bundle_sha256']==gates['bundle_sha256'],'old source/manifest mismatch')
        require(manifest['retire']==self.p['retire'],'retirement differs from verified original manifest')
        archives={manifest['main']['archive_tag']:manifest['main']['expected_sha'],
                  manifest['transport']['archive_tag']:gates['transport_sha']}
        archives.update({r['archive_tag']:r['expected_sha'] for r in manifest['retire']})
        require(archives==self.p['canonical_archives'],'archive plan differs from old proof')
        rows=gates.get('gates',[])
        require([r.get('id') for r in rows]==GATES,'proof is missing or reorders a gate')
        require([r['id'] for r in self.p['gate_contracts']]==GATES,'invalid pinned gate contracts')
        for row,contract in zip(rows,self.p['gate_contracts']):
            require(all(row.get(k)==v for k,v in contract.items()),'proof command contract differs')
            require(row.get('status')=='Pass' and row.get('exit_code')==0 and not row.get('error'),'gate did not pass')
            require(row.get('log')==row['id']+'.log' and sha(files[row['log']])==row.get('log_sha256'),'gate log checksum mismatch')
        consumer=json.loads(files['consumer208.json'])
        require(sha(files['consumer208.json'])==rows[4].get('summary_sha256') and
                consumer.get('status')=='Pass' and consumer.get('source_commit')==gates['source_sha'],'consumer proof mismatch')
        self.proof_receipt={'status':'Pass','proof_sha256':sha(data),
            'executed_source_commit':gates['source_sha'],'executed_source_tree':gates['source_tree'],
            'original_run_id':gates['run_id'],'original_attempt':gates['run_attempt'],
            'original_transport_sha':gates['transport_sha'],'tests_relabelled_or_reexecuted':False}
        save(self.out/'proof-verification.json',self.proof_receipt)

    def source(self):
        p=self.p;source=p['proof_identity']['source_sha'];main=p['expected_main']
        require(self.git('rev-parse',source+'^{tree}').stdout.strip()==p['proof_identity']['source_tree'],'tested source tree mismatch')
        require(self.git('rev-parse',main+'^{tree}').stdout.strip()==p['expected_main_tree'],'current main tree differs')
        require(self.git('rev-list','--parents','-n','1',main).stdout.split()==[main,source],'main is not sole CI-only child of tested source')
        require(self.git('diff','--name-only',source,main).stdout.splitlines()==[p['workflow_path']],'main has additional source changes')
        require(self.git('rev-parse',main+':'+p['workflow_path']).stdout.strip()==p['workflow_blob'],'unreviewed CI workflow blob')
        for row in p['retire']:
            ancestor=self.git('merge-base','--is-ancestor',row['expected_sha'],main,allowed=(0,1)).returncode==0
            require(ancestor==(row['disposition']=='merged'),'retirement ancestry policy mismatch: '+row['ref'])

    def prepare(self):
        self.proof();before=self.snapshot()
        wanted={**self.expected,**self.p['canonical_archives']}
        self.git('fetch','--no-tags','origin',*[f'+{ref}:refs/cleanup213/observed/{i}' for i,ref in enumerate(sorted(wanted))])
        for i,ref in enumerate(sorted(wanted)):
            require(self.git('rev-parse',f'refs/cleanup213/observed/{i}').stdout.strip()==wanted[ref],'ref changed while fetching')
        self.source();self.check_clean();self.snapshot()
        save(self.out/'prepared.json',{'status':'Pass','plan_sha256':sha(self.plan_bytes),
            'expected_heads':self.expected,'canonical_archives':self.p['canonical_archives'],
            'main_unchanged':self.p['expected_main'],'cleanup_transport':self.args.transport_sha,
            'proof':self.proof_receipt,'remote_writes':False})

    def guard(self):
        tree=self.git('mktree',data='').stdout.strip()
        parents=sorted(set(self.expected.values())|set(self.p['canonical_archives'].values())|
                       {self.p['proof_identity']['source_sha']})
        argv=['commit-tree',tree]
        for parent in parents:argv+=['-p',parent]
        env=os.environ.copy()
        for role in ['AUTHOR','COMMITTER']:
            env['GIT_'+role+'_NAME']='github-actions[bot]'
            env['GIT_'+role+'_EMAIL']='41898282+github-actions[bot]@users.noreply.github.com'
            env['GIT_'+role+'_DATE']=self.p['guard_timestamp']
        return self.git(*argv,data='Archive-only cleanup guard; never merge into main.\n'+json.dumps({
            'plan_sha256':sha(self.plan_bytes),'original_heads':self.expected,'old_archives':self.p['canonical_archives'],
            'proof_sha256':self.p['proof_sha256']},sort_keys=True)+'\n',env=env).stdout.strip()

    def execute(self):
        self.prepare();guard=self.guard();self.check_clean();self.snapshot()
        p=self.p
        updates=[(p['main_ref'],p['expected_main'],p['expected_main'])]
        updates += [(r,s,None) for r,s in sorted(self.expected.items()) if r!=p['main_ref']]
        updates += [(r,s,s) for r,s in sorted(p['canonical_archives'].items())]
        updates += [(p['cleanup_tag'],None,self.args.transport_sha),(p['guard_tag'],None,guard)]
        # Main and old tags are no-op refspecs only. Advertisement drift is checked;
        # Git may omit no-ops from its transaction, hence the mandatory post-read.
        argv=['git','push','--atomic','--porcelain']
        argv += ['--force-with-lease='+r+':'+(old or '') for r,old,new in updates]
        argv += ['origin']+[(new or '')+':'+r for r,old,new in updates]
        result=subprocess.run(argv,cwd=self.repo,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        (self.out/'push.stdout.log').write_text(redact(result.stdout))
        (self.out/'push.stderr.log').write_text(redact(result.stderr))
        save(self.out/'push.json',{'exit_code':result.returncode,'argv':argv,
            'stdout_sha256':sha((self.out/'push.stdout.log').read_bytes()),
            'stderr_sha256':sha((self.out/'push.stderr.log').read_bytes())})
        require(result.returncode==0,'atomic cleanup refused; see both push stdout and stderr logs')
        after=self.remote();save(self.out/'remote-after.json',after)
        require({r:s for r,s in after.items() if r.startswith('refs/heads/')}=={p['main_ref']:p['expected_main']},'post-cleanup heads drift; inspect remote, guard preserves deleted history')
        require(all(after.get(r)==s for r,s in p['canonical_archives'].items()),'post-cleanup canonical archive drift; guard preserves history')
        require(after.get(p['cleanup_tag'])==self.args.transport_sha and after.get(p['guard_tag'])==guard,'post-cleanup new archive mismatch')
        return {'status':'Pass','main_written':False,'main_sha':p['expected_main'],
            'retired':{r:s for r,s in self.expected.items() if r!=p['main_ref']},
            'canonical_archives':p['canonical_archives'],'cleanup_tag':p['cleanup_tag'],
            'archive_guard':{'ref':p['guard_tag'],'sha':guard},'proof':self.proof_receipt,
            'atomic':True,'no_non_atomic_fallback':True}

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode',choices=['verify','execute'])
    for name in ['repo','plan','proof','repository','transport-sha','out']:parser.add_argument('--'+name,required=True)
    args=parser.parse_args();out=Path(args.out).resolve();out.mkdir(parents=True,exist_ok=True)
    try:
        cleanup=Cleanup(args)
        if args.mode=='verify':cleanup.prepare();result={'status':'Pass','remote_writes':False}
        else:result=cleanup.execute()
        save(out/'result.json',result);print(json.dumps(result));return 0
    except (Refusal,OSError,ValueError,KeyError,zipfile.BadZipFile) as error:
        save(out/'result.json',{'status':'Fail','error_type':type(error).__name__,'error':redact(str(error)),
            'remote_writes_may_have_completed':(out/'push.json').exists(),'no_non_atomic_fallback':True})
        print('CLEANUP REFUSED: '+redact(str(error)),file=sys.stderr);return 1

if __name__=='__main__':raise SystemExit(main())

#!/usr/bin/env python3
"""Current-source ThinLTO corruption, composition and opaque-input controls."""
import argparse,hashlib,importlib.util,json,subprocess,sys
from pathlib import Path

p=argparse.ArgumentParser();p.add_argument('--evidence-root',type=Path,required=True);p.add_argument('--controls',type=Path,required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args()
root=a.evidence_root.resolve();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False);here=Path(__file__).resolve().parent
sys.path.insert(0,str(here.parent));import stage6_bench as bench;import stage6_runtime as runtime
common=[sys.executable,str(here/'run_request.py'),'--zil',str(root/'target-thin/release/zil'),'--clang','/workspace/.zil-env/llvm-23.1.4/bin/clang','--lld','/workspace/.zil-env/llvm-23.1.4/bin/ld.lld','--snapshot-receipt',str(root/'thin-snapshot/RESEARCH.json')]
rows=[]
def request(name,source,cache,flags,expected,native_input=None):
 dest=out/name;cmd=common+['--source',str(source),'--cache',str(cache),'--out',str(dest)]+flags
 if native_input:cmd+=['--native-input',str(native_input)]
 run=subprocess.run(cmd,capture_output=True);(out/(name+'.stdout')).write_bytes(run.stdout);(out/(name+'.stderr')).write_bytes(run.stderr)
 assert run.returncode==0,(name,run.stdout,run.stderr)
 result=json.loads((dest/'result.json').read_text());assert result['driver_success']
 assert result['native']['exit']==0 and result['native']['stdout_hex']==expected.hex() and not result['native']['stderr_hex'],(name,result['native'])
 inputs=[json.loads(f.read_text()) for f in dest.glob('unit-*.input.json')]
 row={'name':name,'units':len(inputs),'pre_hits':sum(i['pre_cache_hit'] for i in inputs),'post_corrupt_skipped':result['post_corrupt_skipped'],'post_keys':sorted(k for k in result['post_after'] if k.startswith('llvmcache-')),'native':result['native']};rows.append(row);print(json.dumps(row),flush=True)
 return dest,result,row

# Modify disposable local research cache bytes, never compiler source or outputs.
case={'name':'scalar-600','sizes':[600],'flavor':'scalar'};source=out/'source';expected=bench.generate(case,source,0).to_bytes(4,'little')
cache=a.controls.resolve()/'thin-closed0-dso1-cache'
prior=a.controls.resolve()/'thin-closed0-dso1-restore'
key=json.loads(next(prior.glob('unit-*.input.json')).read_text())['action_key']
pre=cache/'pre'/key/'bc';assert pre.is_file();old_sha=hashlib.sha256(pre.read_bytes()).hexdigest();pre.write_bytes(b'corrupt-pre-lto-control')
dest,result,row=request('damaged-pre',source/'main.zil',cache,['--mode','thin','--dso-local'],expected)
assert row['pre_hits']==2 and hashlib.sha256(pre.read_bytes()).hexdigest()==old_sha,row
# The cache also retains historical edit keys. Select a key produced by the
# actual current cold source, not an arbitrary historical entry that is unused.
cold=json.loads((a.controls.resolve()/'thin-closed0-dso1-cold/result.json').read_text())
post_name=next(n for n in cold['post_after'] if n.startswith('llvmcache-'))
post_sha=result['post_after'][post_name]['sha256']
post=cache/'post'/post_name;post.write_bytes(b'corrupt-post-lto-control')
dest,result,row=request('damaged-post',source/'main.zil',cache,['--mode','thin','--dso-local'],expected)
assert row['pre_hits']==3 and post_name in row['post_corrupt_skipped'],row
assert result['post_after'][post_name]['sha256']==post_sha, 'required post key was not rebuilt'
manifest=json.loads((cache/'post-manifest.json').read_text())
for name,identity in manifest.items():assert hashlib.sha256((cache/'post'/name).read_bytes()).hexdigest()==identity['sha256']

# Composition with the existing hand-import strategy is explicit, never inferred.
for mode in ['baseline','thin']:
 for closed in [False,True]:
  name=f'imports-{mode}-{int(closed)}';flags=['--mode',mode,'--imports','on','--dso-local']+(['--closed'] if closed else [])
  dest,_,_=request(name,source/'main.zil',out/(name+'-cache'),flags,expected)
  assert any('define available_externally ' in f.read_text() for f in (dest/'units').glob('unit-*.ll'))

# Opaque input keeps helpers live through all optimizers; check five seeds.
rounds=20
for flavor in ['arithmetic','branch','body']:
 src=out/flavor;src.mkdir();runtime.generate(src,512,flavor,rounds)
 for mode in ['baseline','thin']:
  for closed in [False,True]:
   name=f'opaque-{flavor}-{mode}-{int(closed)}';flags=['--mode',mode,'--dso-local']+(['--closed'] if closed else [])
   expected_by_seed={}
   for seed in [0,1,7,42,255]:
    value=0;x=seed
    for _ in range(rounds):
     total=sum((x+64+i%97)%1009 if flavor=='body' else (x*3+i%97)%1009 if flavor!='branch' or x%3==0 else (x+i%31)%1009 for i in range(512))
     value=(value+total)%1000003;x=(x+17)%1009
    expected_by_seed[seed]=value.to_bytes(4,'little')
   inp=src/'input';inp.write_bytes(bytes([7]));dest,_,_=request(name,src/'main.zil',out/(name+'-cache'),flags,expected_by_seed[7],inp)
   natives=[]
   for seed,expected in expected_by_seed.items():
    native=subprocess.run([dest/'program'],input=bytes([seed]),capture_output=True)
    assert native.returncode==0 and native.stdout==expected and not native.stderr,(name,seed,native)
    natives.append({'seed':seed,'expected_hex':expected.hex(),'stdout_hex':native.stdout.hex(),'exit':native.returncode})
   (dest/'native-seeds.json').write_text(json.dumps(natives,indent=2)+'\n')
(out/'summary.json').write_text(json.dumps({'functional_only':True,'rows':rows,'runner_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()},indent=2)+'\n')

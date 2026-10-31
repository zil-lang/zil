from pathlib import Path
import sys,json,subprocess,importlib.util,hashlib,argparse
parser=argparse.ArgumentParser();parser.add_argument('--evidence-root',type=Path,required=True);parser.add_argument('--out',type=Path,required=True);args=parser.parse_args()
repo=Path(__file__).resolve().parents[4];root=args.evidence_root.resolve();tool=Path(__file__).with_name('run_request.py')
spec=importlib.util.spec_from_file_location('bench',repo/'tools/research/incremental-compilation/stage6_bench.py');bench=importlib.util.module_from_spec(spec);spec.loader.exec_module(bench)
out=args.out.resolve();out.mkdir(parents=True,exist_ok=False);rows=[]
common=[sys.executable,str(tool),'--zil',str(root/'target-thin/release/zil'),'--clang','/workspace/.zil-env/llvm-23.1.4/bin/clang','--lld','/workspace/.zil-env/llvm-23.1.4/bin/ld.lld','--snapshot-receipt',str(root/'thin-snapshot/RESEARCH.json')]
def request(name,source,cache,flags,expected):
 dest=out/name;cmd=common+['--source',str(source),'--cache',str(cache),'--out',str(dest)]+flags
 r=subprocess.run(cmd,capture_output=True,text=True);(out/(name+'.runner.stdout')).write_text(r.stdout);(out/(name+'.runner.stderr')).write_text(r.stderr)
 result=json.loads((dest/'result.json').read_text()) if (dest/'result.json').exists() else {}
 assert r.returncode==0,(name,r.stdout,r.stderr)
 assert result['native']['exit']==0 and result['native']['stderr_hex']=='' and bytes.fromhex(result['native']['stdout_hex'])==expected,(name,result.get('native'))
 inputs=[json.loads(p.read_text()) for p in dest.glob('unit-*.input.json')]
 row={'name':name,'units':len(inputs),'pre_hits':sum(i['pre_cache_hit'] for i in inputs),'post_keys':len([n for n in result['post_after'] if n.startswith('llvmcache-')]),'native':result['native'],'config':flags}
 rows.append(row);print(json.dumps(row),flush=True);return result,row
case={'name':'scalar-600','sizes':[600],'flavor':'scalar'};source=out/'source'
for mode in ['baseline','thin']:
 for closed in [False,True]:
  for dso in [False,True]:
   arm=f'{mode}-closed{int(closed)}-dso{int(dso)}';flags=['--mode',mode]+(['--closed'] if closed else [])+(['--dso-local'] if dso else []);cache=out/(arm+'-cache')
   for label,edit in [('cold',0),('warm',0),('early',1),('late',2),('restore',0)]:
    expected=bench.generate(case,source,edit);result,row=request(arm+'-'+label,source/'main.zil',cache,flags,expected.to_bytes(4,'little'))
    assert row['units']==3
    if label in ['warm','restore']:assert row['pre_hits']==3,(arm,label,row)
    if label in ['early','late']:assert row['pre_hits']==2,(arm,label,row)
   symbols=subprocess.check_output(['nm','--defined-only',str(out/(arm+'-restore')/'program')],text=True)
   count=sum(line.split()[-1].startswith('zil.fn.') and line.split()[-2]=='T' for line in symbols.splitlines() if len(line.split())>=3)
   if not closed:assert count==602,(arm,count)
   if closed and mode=='thin':assert count==0,(arm,count)
   before={str(p.relative_to(cache)):hashlib.sha256(p.read_bytes()).hexdigest() for p in cache.rglob('*') if p.is_file()}
   p=source/'main.zil';p.write_text(p.read_text().replace('h300(0)','h300(0,1)'))
   dest=out/(arm+'-bad');cmd=common+['--source',str(p),'--cache',str(cache),'--out',str(dest)]+flags
   r=subprocess.run(cmd,capture_output=True,text=True);assert r.returncode!=0,(arm,r.stdout)
   report=json.loads((dest/'driver.stdout').read_text());assert report['status']=='SafeReject' and report['phase']=='types',report
   after={str(p.relative_to(cache)):hashlib.sha256(p.read_bytes()).hexdigest() for p in cache.rglob('*') if p.is_file()};assert before==after,(arm,'failed request modified persistent cache')
   rows.append({'name':arm+'-bad','status':'SafeReject','cache_unchanged':True,'default_external_symbols':count})
# Source resource/owned closure/function-address case already accepted by prior driver control.
source=out/'resource.zil';text='extern "C" fn putchar(n:i32)->i32;\nresource Token{code:i32}\nimpl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}\nfn make(n:i32)->Token contract{result(create)}{return Token{code:n}}\nfn early()->i32{let t=make(65);let f=move()=>t.code;return f()-65;}\n'
text+=''.join(f'fn padding{i}(x:i32)->i32{{return x+1;}}\n' for i in range(600))
text+='fn late()->i32{let t=make(66);let f=move()=>t.code;let p:fn(i32)->i32=padding0;return f()+p(0)-67;}\nfn main()->i32{return early()+late();}\n';source.write_text(text)
for mode in ['baseline','thin']:
 for closed in [False,True]:
  name=f'resource-{mode}-closed{int(closed)}';request(name,source,out/(name+'-cache'),['--mode',mode,'--dso-local']+(['--closed'] if closed else []),b'AB')
(out/'summary.json').write_text(json.dumps({'functional_only':True,'rows':rows},indent=2)+'\n')

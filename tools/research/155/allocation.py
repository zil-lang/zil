"""Native owning-local replacement with real allocator failure/leak probes."""
import argparse,pathlib,subprocess,json,os,shutil
p=argparse.ArgumentParser();p.add_argument('--repo',required=True);p.add_argument('--cli',required=True);p.add_argument('--out',required=True);a=p.parse_args();repo=pathlib.Path(a.repo);out=pathlib.Path(a.out);out.mkdir(parents=True,exist_ok=False)
helper='''fn make(n:i64,tag:u8)->ByteBuffer contract{result(give)}{let b=bytes_new();if !bytes_reserve(b,n){return give b};for i in 0..n{bytes_push(b,tag)};return give b}\n'''
programs={
'replace':helper+'''fn main()->i32{var b=make(8,1u8);for n in 0..64{b=make(16+i64(n),u8(n));let v=bytes_view(b);if v.len()!=0 && v.len()!=16+i64(n){return 1};for i in 0..v.len(){if v[i]!=u8(n){return 2}}};return 0}''',
'callback':'''fn make()->impl FnMut(i32)->i32{var b=bytes_new();return move(n:i32)->i32=>{if n==0{b=bytes_new();return 0};if !bytes_reserve(b,64){return -1};bytes_push(b,u8(n));return i32(bytes_len(b))}} fn main()->i32{var f=make();for i in 0..64{let n=f(42);if n!=1 && n!= -1{return 1};if f(0)!=0{return 2}};return 0}''',
'propagated-error':helper+'''enum E{Fail} fn fail()->E!i32{return E.Fail} fn run()->E!i32{var b=make(64,42u8);b=make(i64(fail()?),7u8);return 0} fn main()->E!i32{return run()?}'''
}
rows=[]
for name,source in programs.items():
 src=out/(name+'.zil');src.write_text(source)
 for opt in ['O0','O2']:
  d=out/f'{name}-{opt}';d.mkdir();cmd=[a.cli,'build',str(src),'--opt',opt,'-o',str(d/'program'),'--emit-dir',str(d/'phases'),'--json'];r=subprocess.run(cmd,capture_output=True,timeout=60);(d/'build.command.json').write_text(json.dumps(cmd));(d/'build.stdout').write_bytes(r.stdout);(d/'build.stderr').write_bytes(r.stderr);report=json.loads(r.stdout);assert report['status']=='Success',report
  exe=d/'probe';cmd=['clang','-'+opt,str(d/'phases/llvm.ll'),str(repo/'tools/research/142/allocator_probe.c'),'-Wl,--wrap=realloc','-Wl,--wrap=free','-o',str(exe)];r=subprocess.run(cmd,capture_output=True,timeout=30);(d/'link.command.json').write_text(json.dumps(cmd));(d/'link.stderr').write_bytes(r.stderr);assert r.returncode==0,r.stderr
  fails=[0,1] if name=='propagated-error' else [0,1,2,17,32,64]
  for fail in fails:
   r=subprocess.run([str(exe)],capture_output=True,timeout=10,env={**os.environ,'ZIL_TEST_FAIL_AT':str(fail)});(d/f'fail{fail}.stdout').write_bytes(r.stdout);(d/f'fail{fail}.stderr').write_bytes(r.stderr)
   probe=json.loads(r.stderr);expected=1 if name=='propagated-error' else 0
   ok=r.returncode==expected and probe['live']==0 and probe['allocated']==probe['frees'] and probe['failures']==int(fail>0)
   if name=='replace' and fail==0:ok &= probe['peak_live']==2 and probe['alloc_calls']==65
   row={'case':name,'opt':opt,'fail_at':fail,'exit':r.returncode,'probe':probe,'pass':ok};rows.append(row);(out/'rows.json').write_text(json.dumps(rows,indent=2));assert ok,row
summary={'status':'Pass','native_executions':len(rows),'actual_faults':sum(x['probe']['failures']for x in rows),'rows':rows,'contract':'Staged RHS coexists with old buffer until replacement commit; explicit reset releases immediately. Failure injection returns empty in make; no claim of transactional fallible owned result type.'};(out/'summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))

"""Bounded ZUI admission probes. A checked failure is still a product gap."""
import pathlib,json,subprocess,argparse,os
p=argparse.ArgumentParser();p.add_argument('--repo',required=True);p.add_argument('--cli',required=True);p.add_argument('--out',required=True);a=p.parse_args();repo=pathlib.Path(a.repo);out=pathlib.Path(a.out);out.mkdir(parents=True,exist_ok=False);rows=[]
gaps={
'nested-owning-callback':'resource R{n:i32} fn main()->i32{let a=R{n:3};let f=move()->i32=>{let g=move()->i32=>a.n;return g()};return f()-3}',
'owned-array-elements':'resource R{n:i32} fn main()->i32{let a:[R;2]=[R{n:1},R{n:2}];return 0}',
'owned-error-result':'enum E{Fail} fn make()->E!ByteBuffer contract{result(give)}{return bytes_new()} fn main(){}',
'generic-trait-bound':'trait Read{fn read(self:&Self)->i32;}struct C{x:i32}impl Read for C{fn read()->i32=>.x}fn read<T:Read>(x:&T)->i32=>x.read();fn main(){}',
'owning-method-receiver':'trait Read{fn read(self:&Self)->i32;}resource C{x:i32}impl Read for C{fn read()->i32=>.x}fn main(){}',
'array-of-capturing-callbacks':'fn make(n:i32)->impl FnMut()->i32{var x=n;return move()->i32=>{x+=1;return x}}fn main(){let handlers=[make(1),make(2)]}'
}
for name,text in gaps.items():
 d=out/name;d.mkdir();src=d/'main.zil';src.write_text(text);cmd=[a.cli,'check',str(src),'--json'];r=subprocess.run(cmd,capture_output=True,timeout=30);(d/'command.json').write_text(json.dumps(cmd));(d/'compile.stdout').write_bytes(r.stdout);(d/'compile.stderr').write_bytes(r.stderr);j=json.loads(r.stdout)
 rows.append({'capability':name,'status':j['status'],'phase':j['phase'],'diagnostics':j['diagnostics'],'language_gap':j['status']!='Success'});assert j['phase']!='parse',(name,j)
consumer=repo/'examples/mainline/zui_boundary159/main.zil';native=[]
for opt in ['O0','O2']:
 d=out/opt;d.mkdir();cmd=[a.cli,'build',str(consumer),'--opt',opt,'-o',str(d/'program'),'--emit-dir',str(d/'phases'),'--json'];r=subprocess.run(cmd,capture_output=True,timeout=60);(d/'build.command.json').write_text(json.dumps(cmd));(d/'build.stdout').write_bytes(r.stdout);(d/'build.stderr').write_bytes(r.stderr);j=json.loads(r.stdout);assert j['status']=='Success',j;assert j['verification_runs']==1
 r=subprocess.run([str(d/'program')],capture_output=True,timeout=15);(d/'normal.stdout').write_bytes(r.stdout);(d/'normal.stderr').write_bytes(r.stderr);assert r.returncode==0,r
 exe=d/'probe';cmd=['clang','-'+opt,str(d/'phases/llvm.ll'),str(repo/'tools/research/142/allocator_probe.c'),'-Wl,--wrap=realloc','-Wl,--wrap=free','-o',str(exe)];r=subprocess.run(cmd,capture_output=True,timeout=30);(d/'probe-link.command.json').write_text(json.dumps(cmd));(d/'probe-link.stderr').write_bytes(r.stderr);assert r.returncode==0,r.stderr
 r=subprocess.run([str(exe)],capture_output=True,timeout=15);(d/'probe.stdout').write_bytes(r.stdout);(d/'probe.stderr').write_bytes(r.stderr);probe=json.loads(r.stderr);assert r.returncode==0 and probe['live']==0 and probe['allocated']==probe['frees']==5 and probe['peak_live']==2,probe
 native.append({'opt':opt,'native_executions':2,'events_per_execution':16384,'full_byte_checks_per_execution':16384,'probe':probe,'pass':True})
s={'scope':'headless queue + imported methods/trait + captured owning local reset','status':'CoreConsumerPass_FullZuiNotReady','native':native,'gaps':rows,'not_run':['actual window/GPU backend','cross-thread callback cancellation','dynamic retained graph with owning generic components'],'not_claimed':'No universal Vec<T>, nested owning callbacks, complete trait solver, or full language safety proof'};(out/'summary.json').write_text(json.dumps(s,indent=2));print(json.dumps(s,indent=2))

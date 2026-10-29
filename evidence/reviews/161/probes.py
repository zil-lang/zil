import pathlib,subprocess,json,hashlib
root=pathlib.Path('/workspace/scratch/39be3faa4442/run-logs/review161')
cli=pathlib.Path('/workspace/scratch/39be3faa4442/targets/review161/debug/zil')
header='extern "C" fn putchar(x:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}'
positive={
 'triple_nonmove_to_deep_move':(header+'fn main()->i32{let a=R{code:65};let f=()=>{let g=()=>{let h=move()=>a.code;return h()};return g()};return f()-65}',b'A',0),
 'resource_shadow_does_not_steal_outer':(header+'fn main()->i32{let a=R{code:65};let f=()=>{let g=move()=>{let a=R{code:66};let h=move()=>a.code;return h()};return g()+a.code};return f()+f()-262}',b'BBA',0),
 'middle_snapshot_deep_borrowed_write':('fn main()->i32{var x=40;let f=()=>{var g=move()=>{var h=()=>{x+=1;return x};return h()+h()};return g()+g()};if f()!=170||f()!=170{return 1};return x-40}',b'',0),
 'deep_unused_environment':(header+'fn main()->i32{let a=R{code:65};let f=()=>{let g=()=>{let h=move()=>a.code;return 0};return 0};return f()}',b'A',0),
 'descriptor_three_levels_repeat':(header+'fn main()->i32{let a=R{code:65};let f=()=>{let g=()=>{let h=()=>a.code;return h()+h()};return g()+g()};return f()+f()-520}',b'A',0),
 'inner_prepare_error_drop_order':(header+'enum E{Fail}fn fail()->E!i32=>E.Fail;fn main()->E!i32{let a=R{code:65};let f=()=>{let b=R{code:66};let g=move()->E!i32=>{let c=R{code:67};let n=a.code;return fail()?};return g()?};return f()?}',b'CAB',1),
 'triple_package_nonmove_transfer':(header+'package P{manages a:R}fn main()->i32{let p=P{a:R{code:65}};let f=()=>{let g=()=>{let h=move()=>p.a.code;return h()};return g()};return f()-65}',b'A',0),
}
negative={
 'late_descriptor_after_deep_drop':header+'fn main()->i32{let a=R{code:65};let alias=a;let f=()=>{let g=()=>{let h=move()=>a.code;return h()};return g()};let n=f();return alias.code-n}',
 'late_slice_after_deep_drop':'fn main()->i32{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,42u8);let view=bytes_view(b);let f=()=>{let g=move()=>bytes_len(b);return g()};let n=f();return i32(view[0])+i32(n)}',
 'deep_consumed_outer_in_loop':header+'fn main()->i32{let a=R{code:65};let f=()=>{let g=()=>{let h=move()=>a.code;return h()};return g()};for i in 0..2{f()};return 0}',
 'conditional_package_refill_is_once':header+'package P{manages a:R}fn main()->i32{var p=P{a:R{code:65}};var f=move(flag:bool)=>{let g=move()=>p.a.code;if flag{p=P{a:R{code:66}}};return g()};return f(true)+f(true)-130}',
 'move_snapshot_cannot_grant_write_to_shared_address':'fn main()->i32{var x=1;let a=&x;let f=move()=>{let g=move()=>{*a=2;return *a};return g()};return f()}',
}
rows=[]
for name,(source,stdout,exitcode) in positive.items():
 d=root/name;d.mkdir(exist_ok=True);s=d/'main.zil';s.write_text(source)
 for opt in ('O0','O2'):
  exe=d/opt;r=subprocess.run([str(cli),'build',str(s),'--opt',opt,'-o',str(exe),'--emit-dir',str(d/(opt+'-phases')),'--json'],capture_output=True,timeout=60)
  (d/(opt+'-build.stdout')).write_bytes(r.stdout);(d/(opt+'-build.stderr')).write_bytes(r.stderr)
  report=json.loads(r.stdout);row={'name':name,'opt':opt,'kind':'positive','compile':report['status'],'phase':report['phase'],'verification_runs':report['verification_runs'],'diagnostics':report['diagnostics']}
  if r.returncode==0 and report['status']=='Success':
   run=subprocess.run([str(exe)],capture_output=True,timeout=15);(d/(opt+'-stdout')).write_bytes(run.stdout);(d/(opt+'-stderr')).write_bytes(run.stderr)
   row.update(exit_code=run.returncode,stdout=run.stdout.decode(errors='replace'),pass_result=run.returncode==exitcode and run.stdout==stdout)
  else:row['pass_result']=False
  rows.append(row);print(json.dumps(row),flush=True)
for name,source in negative.items():
 d=root/name;d.mkdir(exist_ok=True);s=d/'main.zil';s.write_text(source)
 r=subprocess.run([str(cli),'check',str(s),'--json'],capture_output=True,timeout=30);(d/'check.stdout').write_bytes(r.stdout);(d/'check.stderr').write_bytes(r.stderr)
 report=json.loads(r.stdout);row={'name':name,'kind':'negative','compile':report['status'],'phase':report['phase'],'diagnostics':report['diagnostics'],'pass_result':r.returncode!=0 and report['status']=='SafeReject' and report['phase']!='parse'}
 rows.append(row);print(json.dumps(row),flush=True)
(root/'probes-result.json').write_text(json.dumps(rows,indent=2)+'\n')
assert all(r['pass_result']for r in rows),'review failure; see probes-result.json'

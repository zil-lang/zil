from pathlib import Path
import hashlib, json, subprocess
out=Path(__file__).parent
cli=Path('/workspace/scratch/39be3faa4442/targets/review169-final/debug/zil')
source='33ac699268f0b2466a5fa79e9d53007ac54984f3'
prefix='''extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn make(n:i32)->impl FnOnce()->i32{let r=R{code:n};return move()=>{let own=give r;return own.code}}
enum E{Fail} fn fail()->E!i32{return E.Fail}
'''
cases={
 'once_move_refill_late_guard': ('''fn main()->i32{var a=[make(65),make(66),make(67)];let later=R{code:90};let old=a[1];a[1]=make(68);if old()!=66{return 1};let b=a;let tail=b[2];if tail()!=67{return 2};return later.code-90}''', 'Success','BCADZ',0),
 'failed_refill_late_guard': ('''fn main()->E!i32{var a=[make(65),make(66),make(67)];let later=R{code:90};a[1]=make(fail()?);return later.code-90}''', 'Success','ZABC',1),
 'partial_whole_move': ('''fn main()->i32{let a=[make(65),make(66)];let old=a[0];let b=a;return old()-65}''','SafeReject',None,None),
 'take_consumed_element': ('''fn main()->i32{let a=[make(65),make(66)];let first=a[0];let n=first();let again=a[0];return again()-n}''','SafeReject',None,None),
 'capture_whole_array': ('''fn main(){let a=[make(65),make(66)];let saved=move()=>{let first=a[0];first()}}''','Unsupported',None,None),
}
report={'status':'Running','reviewer':'/root','compiler_source_commit':source,'cli_sha256':hashlib.sha256(cli.read_bytes()).hexdigest(),'observations':[]}
try:
 for name,(body,expected,stdout,code) in cases.items():
  file=out/(name+'.zil'); file.write_text(prefix+body+'\n')
  for opt in ('O0','O2') if expected=='Success' else ('O0',):
   executable=out/(name+'-'+opt)
   argv=[str(cli),'build' if expected=='Success' else 'check',str(file),'--json']
   if expected=='Success':argv += ['--opt',opt,'-o',str(executable)]
   result=subprocess.run(argv,capture_output=True,timeout=120)
   (out/(name+'-'+opt+'.json')).write_bytes(result.stdout)
   (out/(name+'-'+opt+'.stderr')).write_bytes(result.stderr)
   checked=json.loads(result.stdout)
   row={'case':name,'command':argv,'source_sha256':hashlib.sha256(file.read_bytes()).hexdigest(),'expected':expected,'status':checked['status'],'diagnostics':checked.get('diagnostics',[]),'verification_runs':checked['verification_runs']}
   report['observations'].append(row)
   assert checked['status']==expected, row
   if expected=='Success':
    assert result.returncode==0 and checked['verification_runs']==1, row
    run=subprocess.run([str(executable)],capture_output=True,timeout=20)
    row.update(stdout=run.stdout.decode(),exit_code=run.returncode,stderr=run.stderr.decode())
    assert run.stdout.decode()==stdout and run.returncode==code, row
 report['status']='Pass'
except Exception as e:
 report.update(status='Fail',error=repr(e)); raise
finally:
 (out/'summary.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
 print(json.dumps(report,ensure_ascii=False,indent=2))

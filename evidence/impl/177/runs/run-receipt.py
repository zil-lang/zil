from pathlib import Path
import argparse,os,json,subprocess,time,hashlib,re,sys
p=argparse.ArgumentParser();p.add_argument('--out',required=True);p.add_argument('--target');p.add_argument('--impl');p.add_argument('--role',default='/root');p.add_argument('command',nargs=argparse.REMAINDER);a=p.parse_args();out=Path(a.out).resolve();out.mkdir(parents=True,exist_ok=False)
cmd=a.command[1:] if a.command and a.command[0]=='--' else a.command
env=os.environ.copy();env.update(CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',ZIL_TEST_REPORT_DIR=str(out/'cases'))
if a.target:env['CARGO_TARGET_DIR']=a.target
record={'actor':a.role,'cwd':str(Path.cwd()),'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'crates_tree':subprocess.check_output(['git','rev-parse','HEAD:crates'],text=True).strip(),'source_clean':not subprocess.check_output(['git','status','--porcelain'],text=True).strip(),'command':cmd,'environment':{k:env[k] for k in ['CARGO_TARGET_DIR','CARGO_INCREMENTAL','CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG','ZIL_TEST_REPORT_DIR'] if k in env},'start':time.time(),'status':'Running'}
if a.impl:record['implementation_commit']=a.impl
(out/'command.json').write_text(json.dumps(record,indent=2)+'\n')
with (out/'output.log').open('wb') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
raw=(out/'output.log').read_bytes();record.update(status='Pass' if r.returncode==0 else 'Fail',exit_code=r.returncode,end=time.time(),log_sha256=hashlib.sha256(raw).hexdigest(),test_groups=re.findall(r'test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored',raw.decode(errors='replace')))
(out/'command.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record));print(raw.decode(errors='replace')[-10000:]);sys.exit(r.returncode)

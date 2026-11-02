#!/usr/bin/env python3
"""One full current-source request with transactional pre/post ThinLTO caches."""
import argparse,hashlib,json,os,shutil,subprocess,time
from pathlib import Path

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def identity(p):
 p=p.absolute();s=p.stat();return {'path':str(p),'resolved_path':str(p.resolve()),'sha256':sha(p),'bytes':s.st_size,'mtime_ns':s.st_mtime_ns}
def inventory(root):
 return {str(p.relative_to(root)):{'bytes':p.stat().st_size,'sha256':sha(p)} for p in sorted(root.rglob('*')) if p.is_file()}
def main():
 p=argparse.ArgumentParser();p.add_argument('--zil',type=Path,required=True);p.add_argument('--clang',type=Path,required=True);p.add_argument('--lld',type=Path,required=True);p.add_argument('--source',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--cache',type=Path,required=True);p.add_argument('--mode',choices=['baseline','thin'],required=True);p.add_argument('--imports',choices=['on','off'],default='off');p.add_argument('--closed',action='store_true');p.add_argument('--dso-local',action='store_true');p.add_argument('--units',type=int,default=8);p.add_argument('--jobs',type=int,default=2);p.add_argument('--native-input',type=Path);p.add_argument('--snapshot-receipt',type=Path,required=True);a=p.parse_args()
 service_start=time.perf_counter_ns()
 a.out=a.out.resolve();a.cache=a.cache.resolve();a.out.mkdir(parents=True,exist_ok=False);a.cache.mkdir(parents=True,exist_ok=True)
 wrapper=Path(__file__).with_name('clang_wrapper.py').resolve();tools={k:identity(getattr(a,k)) for k in ['zil','clang','lld']}
 snapshot=json.loads(a.snapshot_receipt.read_text());assert snapshot['production_mutated'] is False
 (a.out/'snapshot-receipt.json').write_bytes(a.snapshot_receipt.read_bytes())
 c={'mode':a.mode,'dso_local':a.dso_local,'request':str(a.out),'cache':str(a.cache),'tools':tools,'jobs':a.jobs,'snapshot_sha256':sha(a.snapshot_receipt)}
 config=a.out/'config.json';config.write_text(json.dumps(c,indent=2)+'\n')
 post=a.out/'post-cache';post.mkdir();before=inventory(a.cache/'post')
 try:trusted_post=json.loads((a.cache/'post-manifest.json').read_text())
 except (OSError,ValueError):trusted_post={}
 corrupt_post=[]
 for rel in before:
  if trusted_post.get(rel)!=before[rel]:corrupt_post.append(rel);continue
  source=a.cache/'post'/rel;target=post/rel;target.parent.mkdir(parents=True,exist_ok=True)
  try:os.link(source,target)
  except OSError:shutil.copy2(source,target)
 env=os.environ.copy();env['ZIL_THIN_CONFIG']=str(config);env['ZIL_THIN_CAPTURE']=str(a.out/'units')
 if a.imports=='off':env['ZIL_THIN_IMPORTS_OFF']='1'
 else:env.pop('ZIL_THIN_IMPORTS_OFF',None)
 command=[str(a.zil.resolve()),'build',str(a.source.resolve()),'--json','--backend','clang','--clang',str(wrapper),'--opt','O2','--incremental-o2','--codegen-units',str(a.units),'--codegen-jobs',str(a.jobs),'-o',str(a.out/'program')]
 if a.closed:command+=['--closed-executable']
 start=time.perf_counter_ns();run=subprocess.run(command,env=env,capture_output=True);elapsed=time.perf_counter_ns()-start
 (a.out/'driver.stdout').write_bytes(run.stdout);(a.out/'driver.stderr').write_bytes(run.stderr)
 try:report=json.loads(run.stdout)
 except ValueError:report={}
 success=run.returncode==0 and report.get('status')=='Success' and report.get('verification_runs')==1
 result={'command':command,'exit':run.returncode,'elapsed_ns':elapsed,'driver_success':success,'mode':a.mode,'imports':a.imports,'closed':a.closed,'dso_local':a.dso_local,'tools':tools,'snapshot_sha256':sha(a.snapshot_receipt),'pre_cache_committed':False,'post_cache_committed':False,'post_before':before,'post_corrupt_skipped':corrupt_post,'post_after':inventory(post),'scope':'elapsed_ns is driver invocation; compile_service_elapsed_ns additionally includes setup/tool hashing, staged cache copy, audit inventories and transactional cache publication. Native execution excluded from both.'}
 if success:
  for entry in sorted((a.out/'pending').glob('*')) if (a.out/'pending').exists() else []:
   dst=a.cache/'pre'/entry.name;dst.parent.mkdir(exist_ok=True)
   if dst.exists():shutil.rmtree(dst)
   shutil.copytree(entry,dst)
  (a.cache/'post').mkdir(exist_ok=True)
  for rel in result['post_after']:
   # Timestamp file is bookkeeping, never a reusable code object identity.
   if not Path(rel).name.startswith('llvmcache-'):continue
   source=post/rel;dst=a.cache/'post'/rel;dst.parent.mkdir(parents=True,exist_ok=True)
   if dst.exists() and rel not in corrupt_post:assert sha(dst)==sha(source), 'post-LTO cache key collision'
   if not dst.exists() or rel in corrupt_post:shutil.copy2(source,dst)
  final_post=inventory(a.cache/'post')
  trusted_updated={k:v for k,v in trusted_post.items() if final_post.get(k)==v}
  trusted_updated.update({k:v for k,v in result['post_after'].items() if Path(k).name.startswith('llvmcache-') and final_post.get(k)==v})
  (a.cache/'post-manifest.json').write_text(json.dumps(trusted_updated,indent=2)+'\n')
  result['pre_cache_committed']=result['post_cache_committed']=True
  result['compile_service_elapsed_ns']=time.perf_counter_ns()-service_start
  native=subprocess.run([str(a.out/'program')],input=a.native_input.read_bytes() if a.native_input else b'',capture_output=True)
  result['native']={'exit':native.returncode,'stdout_hex':native.stdout.hex(),'stderr_hex':native.stderr.hex(),'executable_sha256':sha(a.out/'program')}
 if not success:result['compile_service_elapsed_ns']=time.perf_counter_ns()-service_start
 (a.out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ['post_before','post_after','tools']},indent=2))
 raise SystemExit(0 if success else 1)
if __name__=='__main__':main()

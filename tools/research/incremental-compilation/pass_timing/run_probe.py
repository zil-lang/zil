#!/usr/bin/env python3
"""Profile actual production LLVMRunPasses without changing its pipeline."""
import argparse,hashlib,json,os,re,signal,subprocess,time
from pathlib import Path
from build_probe import identity

def parse_tables(text):
    rows=[];section=None;table=-1
    for line in text.splitlines():
        if 'timing report' in line.lower():section=line.strip();table+=1
        values=list(re.finditer(r'([0-9]+\.[0-9]+)\s+\(\s*([0-9.]+)%\)',line))
        if section and values:
            name=line[values[-1].end():].strip()
            if name:
                rows.append({'table':table,'section':section,'name':name,'columns_seconds':[float(v.group(1)) for v in values],'wall_seconds':float(values[-1].group(1)),'cpu_seconds':float(values[-2].group(1)) if len(values)>=4 else None,'raw':line})
    return rows

def main():
    p=argparse.ArgumentParser();p.add_argument('--zil',type=Path,required=True);p.add_argument('--probe-manifest',type=Path,required=True);p.add_argument('--source',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--merge',choices=['off','after','before-and-after'],default='off');p.add_argument('--closed',action='store_true');p.add_argument('--incremental-o2',action='store_true');p.add_argument('--units',type=int,default=8);p.add_argument('--native-input',type=Path);p.add_argument('--expected-hex');p.add_argument('--timeout',type=int,default=240);p.add_argument('--slp',choices=['on','off']);a=p.parse_args()
    assert not os.environ.get('LD_PRELOAD') and not os.environ.get('LD_AUDIT'),'refuse unrecorded loader interposition'
    a.out=a.out.resolve();a.out.mkdir(parents=True,exist_ok=False);probe=json.loads(a.probe_manifest.read_text())
    for name in ['probe','llvm']:assert identity(Path(probe[name]['path']))==probe[name],name+' identity changed'
    env=os.environ.copy();env.update(LD_PRELOAD=probe['probe']['path'],ZIL_PASS_TIMING_TARGET=str(a.zil.resolve()),ZIL_PASS_TIMING_LLVM=probe['llvm']['path'])
    env.pop('ZIL_PASS_TIMING_SLP',None)
    if a.slp is not None:env['ZIL_PASS_TIMING_SLP']=a.slp
    cmd=[str(a.zil.resolve()),'build',str(a.source.resolve()),'--backend','llvm-api','--opt','O2','--function-merging',a.merge,'--codegen-units',str(a.units),'--codegen-jobs','1','--object-cache',str(a.out/'unused-cache'),'--json','-o',str(a.out/'program')]
    if a.closed:cmd+=['--closed-executable']
    if a.incremental_o2:cmd+=['--incremental-o2']
    receipt={'scope':'Instrumented actual API diagnostic; extra timer overhead, jobs1 and no cache reuse; not formal performance','compiler':identity(a.zil),'probe_manifest':probe,'probe_manifest_sha256':hashlib.sha256(a.probe_manifest.read_bytes()).hexdigest(),'command':cmd,'timeout_seconds':a.timeout,'native_input_sha256':hashlib.sha256(a.native_input.read_bytes()).hexdigest() if a.native_input else None,'expected_hex':a.expected_hex,'global_slp_selection':a.slp,'effective_research_identity':{'production_pipeline':'recorded in result native_codegen','global_vectorize_slp':a.slp,'cache':'disabled by existing loader interposition contract'}}
    (a.out/'request.json').write_text(json.dumps(receipt,indent=2)+'\n')
    start=time.perf_counter_ns();process=subprocess.Popen(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
    timeout=False
    try:stdout,stderr=process.communicate(timeout=a.timeout)
    except subprocess.TimeoutExpired:
        timeout=True;os.killpg(process.pid,signal.SIGKILL);stdout,stderr=process.communicate()
    wall=time.perf_counter_ns()-start;(a.out/'driver.stdout').write_bytes(stdout);(a.out/'driver.stderr').write_bytes(stderr)
    try:report=json.loads(stdout)
    except ValueError:report={}
    cache=(report.get('native_codegen') or {}).get('object_cache') or {}
    tables=parse_tables(stderr.decode(errors='replace'));(a.out/'pass-tables.json').write_text(json.dumps(tables,indent=2)+'\n')
    sources={str(Path(path).resolve()):hashlib.sha256(Path(path).read_bytes()).hexdigest() for path in report.get('source_files',{}).values()}
    result={'exit':process.returncode,'timeout':timeout,'instrumented_wall_ns':wall,'status':report.get('status'),'verification_runs':report.get('verification_runs'),'source_fingerprint':report.get('input_fingerprint'),'source_sha256':sources,'native_codegen':report.get('native_codegen'),'probe_enabled':'[zil-api-pass-timing] enabled llvm=23.1.4' in stderr.decode(errors='replace'),'pass_tables_present':bool(tables),'cache_disabled_by_contract':cache.get('requested') is True and cache.get('enabled') is False and 'dynamic-loader interposition' in (cache.get('disabled_reason') or ''),'native':None}
    if process.returncode==0 and report.get('status')=='Success':
        native=subprocess.run([str(a.out/'program')],input=a.native_input.read_bytes() if a.native_input else b'',capture_output=True,timeout=60)
        result['native']={'exit':native.returncode,'stdout_hex':native.stdout.hex(),'stderr_hex':native.stderr.hex(),'executable':identity(a.out/'program')}
    native=result['native'] or {}
    result['valid']=not timeout and process.returncode==0 and result['verification_runs']==1 and result['probe_enabled'] and result['pass_tables_present'] and result['cache_disabled_by_contract'] and native.get('exit')==0 and native.get('stderr_hex')=='' and (a.expected_hex is None or native.get('stdout_hex')==a.expected_hex)
    if a.slp is not None:result['valid'] &= '[zil-api-pass-timing] vectorize-slp='+a.slp in stderr.decode(errors='replace')
    (a.out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ['native_codegen','source_sha256']},indent=2));raise SystemExit(0 if result['valid'] else 1)

if __name__=='__main__':main()

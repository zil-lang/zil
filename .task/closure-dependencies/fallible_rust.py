#!/usr/bin/env python3
"""Supplemental reference: preserve original Rust and compare recoverable growth.
Not part of the frozen ZIL improvement aggregate; no assertion of identical allocators.
"""
import argparse, importlib.util, json, os, random, statistics, subprocess, time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base',required=True);p.add_argument('--candidate',required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True)
os.sched_setaffinity(0,sorted(os.sched_getaffinity(0))[:2])
spec=importlib.util.spec_from_file_location('matrix',Path(__file__).parent/'bench/measure.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
cases=[c for c in m.corpus.generate(out/'corpus') if c['group']=='resource'];rust=subprocess.check_output(['rustup','which','rustc'],text=True).strip();rows=[]
replacement='''if data.len()==data.capacity() {
            let wanted=if data.capacity()<8 {8} else {
                match data.capacity().checked_mul(2) {Some(n) if n<=isize::MAX as usize=>n,_=>return -1}
            };
            if data.try_reserve_exact(wanted-data.len()).is_err(){return -1;}
        }
        data.push(byte);'''
try:
 for case in cases:
  for block in range(-1,5):
   for request,snapshot in [('cold','base'),('warm','base'),('private','private'),('public','public')]:
    arms=['base','cand','rust','rust_fallible'];random.Random(f'{case["id"]}/{block}/{request}').shuffle(arms)
    for arm in arms:
     work=out/'work'/case['id']/str(block)/arm;work.mkdir(parents=True,exist_ok=True);isrust=arm.startswith('rust');source=work/('main.rs' if isrust else 'main.zil');exe=work/'app'
     text=m.source_text(out/'corpus',case,snapshot,'rust' if isrust else arm)
     if arm=='rust_fallible':
      assert text.count('data.push(byte);')==case['n'];text=text.replace('data.push(byte);',replacement)
     if not source.exists() or source.read_text()!=text:source.write_text(text)
     if isrust:
      argv=[rust,source,'--edition=2024','--crate-name','fallible_comparison','-A','warnings','-C','opt-level=2','-C','debuginfo=0','-C','overflow-checks=yes','-C','panic=abort','-C','embed-bitcode=no','-C','incremental='+str(work/'incremental'),'-o',exe]
     else:
      argv=[a.base if arm=='base' else a.candidate,'build',source,'--json','--opt','O2','--backend','llvm-api','--clang',os.environ['ZIL_CLANG'],'--codegen-units','8','--codegen-jobs','2','--incremental-o2','--function-merging','after','--semantic-cache',work/'semantic','--object-cache',work/'objects','-o',exe]
     row,stdout,stderr=m.timed(argv,out/'logs'/str(len(rows)),timeout=120)
     row.update(case=case['id'],arm=arm,request=request,block=block,warmup=block<0,source_sha256=m.sha(source),checks=[])
     assert row['exit']==0 and not row['timeout'],(row,stdout,stderr)
     if not isrust:
      report=json.loads(stdout);assert report['status']=='Success' and report['verification_runs']==1;row['native_codegen']=report['native_codegen']
     for data in [bytes([1,0,0,1]),bytes([29,2,0,2]),bytes([251,5,0,3])]:
      r=subprocess.run([str(exe)],input=data,capture_output=True,timeout=30);want=m.expected(case,snapshot,data)
      assert r.returncode==0 and r.stdout==want and not r.stderr,(arm,case,snapshot,r.stdout,want,r.stderr)
      row['checks'].append({'input':data.hex(),'actual':r.stdout.hex(),'expected':want.hex()})
     rows.append(row);(out/'rows.jsonl').open('a').write(json.dumps(row)+'\n')
finally:
 cells=[]
 for c in cases:
  for request in ['cold','warm','private','public']:
   cell={'case':c['id'],'request':request}
   for arm in ['base','cand','rust','rust_fallible']:
    values=[r['wall_ms'] for r in rows if r['case']==c['id'] and r['request']==request and r['arm']==arm and not r['warmup']]
    cell[arm]={'n':len(values),'mean_ms':statistics.mean(values) if values else None,'values_ms':values}
   cells.append(cell)
 complete=len(rows)==len(cases)*6*4*4
 (out/'summary.json').write_text(json.dumps({'complete':complete,'requests':len(rows),'formal':sum(not r['warmup'] for r in rows),'native_checks':sum(len(r['checks']) for r in rows),'cells':cells,'scope':'allocation-failure recoverability supplement; same source observable normal outputs, not identical allocator or std implementation; excluded from primary weighted aggregate'},indent=2)+'\n')
 print(json.dumps({'complete':complete,'requests':len(rows)}))

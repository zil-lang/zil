#!/usr/bin/env python3
"""Finite resident-state validation; never a claim of a production byte limit."""
import argparse,json,os,select,signal,statistics,subprocess,sys,time
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parent/'bench'))
import corpus

class Worker:
 def __init__(self,exe,source,root,capacity):
  root.mkdir(parents=True);self.err=open(root/'stderr','wb');self.p=subprocess.Popen([str(exe),'check',str(source),str(root/'unused'),str(root/'cache'),str(capacity)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.err,start_new_session=True,bufsize=0);self.ready=self.read();assert self.ready.get('ready')
 def read(self):
  if not select.select([self.p.stdout],[],[],120)[0]:raise TimeoutError('worker deadline')
  line=self.p.stdout.readline();assert line,'worker EOF';return json.loads(line)
 def ask(self,command):
  t=time.perf_counter_ns();self.p.stdin.write(command.encode()+b'\n');self.p.stdin.flush();r=self.read();r['protocol_wall_ms']=(time.perf_counter_ns()-t)/1e6;return r
 def close(self):
  try:
   if self.p.poll() is None:self.p.stdin.write(b'quit\n');self.p.stdin.flush();self.p.wait(timeout=10)
  finally:
   if self.p.poll() is None:os.killpg(self.p.pid,signal.SIGKILL);self.p.wait()
   self.err.close()

def facts(answer):
 r=answer['report'];return {k:r[k] for k in ('status','phase','diagnostics','input_fingerprint','scalar_interfaces')}

def main():
 p=argparse.ArgumentParser();p.add_argument('--worker',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--rounds',type=int,default=120);a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False);rows=[];cases=[];failures=[];oracles=0
 try:
  for kind,n in [('owned32',32),('shared1024',1024)]:
   for capacity in (8,32,128):
    root=out/f'{kind}-{capacity}';root.mkdir();source=root/'main.zil';seed=(corpus.resources(n,'zil',False) if kind.startswith('owned') else corpus.graph(kind,n,'zil',False))+corpus.io_source('zil');source.write_text(seed)
    worker=Worker(a.worker.resolve(),source,root/'resident',capacity);current=[]
    def save(label,answer):
     path=root/(label+'.json');path.write_text(json.dumps(answer,indent=2));row={'case':kind,'capacity':capacity,'label':label,'rss_kib':answer.get('rss_kib'),'peak_kib':answer.get('peak_kib'),'cached_queries':answer.get('cached_queries'),'status':answer.get('report',{}).get('status'),'wall_ms':answer.get('protocol_wall_ms')};rows.append(row);current.append(row)
    def fresh(label,answer):
     nonlocal oracles
     peer=Worker(a.worker.resolve(),source,root/('fresh-'+label),capacity)
     try:
      other=peer.ask('compile');save('oracle-'+label,other);assert facts(answer)==facts(other),(label,facts(answer),facts(other));oracles+=1
     finally:peer.close()
    try:
     for revision in range(a.rounds):
      if kind.startswith('owned'):
       old='+bytes_len(state.data)+1)%251';assert seed.count(old)==1;text=seed.replace(old,f'+bytes_len(state.data)+{revision+1})%251')
      else:
       old='return (x+1)%65521;';assert old in seed;text=seed.replace(old,f'return (x+{revision+1})%65521;',1)
      source.write_text(text);r=worker.ask('compile');save(f'edit-{revision:04}',r)
      assert r['report']['status']=='Success' and r['report']['verification_runs']==1 and r['active_queries']==0
      assert r['cached_queries']<=capacity
      if r['peak_kib']>2*1024*1024:raise RuntimeError('2 GiB research observation guard exceeded; no production byte guarantee')
      if revision in (0,29,59,89,a.rounds-1):fresh(f'edit-{revision:04}',r)
     for label,text in [('syntax','fn main( {'),('types',seed.replace('let seed=read();','let seed=true;'))]:
      source.write_text(text);r=worker.ask('compile');save(label,r);assert r['report']['status']!='Success';fresh(label,r)
      source.write_text(seed);r=worker.ask('compile');save('restore-'+label,r);assert r['report']['status']=='Success' and r['report']['verification_runs']==1;fresh('restore-'+label,r)
     clear=worker.ask('clear');assert clear.get('cleared') and clear['cached_queries']==0;(root/'clear.json').write_text(json.dumps(clear))
     r=worker.ask('compile');save('after-clear',r);fresh('after-clear',r);assert r['report']['status']=='Success' and r['report']['verification_runs']==1
     edits=[x for x in current if x['label'].startswith('edit-')];last=edits[-30:];prev=edits[-60:-30];ratio=max(x['rss_kib'] for x in last)/max(x['rss_kib'] for x in prev)
     cases.append({'case':kind,'capacity_entries':capacity,'distinct_body_revisions':a.rounds,'max_resident_mib':max(x['rss_kib'] for x in edits)/1024,'peak_process_mib':max(x['peak_kib'] for x in edits)/1024,'last30_mean_ms':statistics.mean(x['wall_ms'] for x in last),'last30_vs_previous30_max_rss_ratio':ratio,'within_10_percent_window_growth':ratio<=1.1,'memory_after_clear_compile_mib':r['rss_kib']/1024,'clear_released_queries':True,'fresh_errors_and_recovery_equal':True})
    finally:worker.close()
 except Exception as e:
  failures.append(repr(e));raise
 finally:
  summary={'scope':'finite 120-body-edit resident experiment; 32-entry matrix not replaced; no production memory byte budget','status':'Pass' if not failures and len(cases)==6 else 'Incomplete','cases':cases,'request_observations':len(rows),'fresh_comparisons':oracles,'failures':failures,'all_caps_covered':len(cases)==6,'main_modified':False};(out/'rows.json').write_text(json.dumps(rows,indent=2));(out/'summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
if __name__=='__main__':main()

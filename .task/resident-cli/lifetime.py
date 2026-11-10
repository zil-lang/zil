#!/usr/bin/env python3
import argparse,json,os,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--cli',required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args();a.out.mkdir(parents=True)
source=a.out/'main.zil'
def program(n):
    return '\n'.join(f'fn f{i}(x:i32)->i32{{if x<10{{return x+{n if i==0 else i};}}else{{return x-1;}}}}' for i in range(256))+'\nfn main()->i32{return f0(1);}\n'
source.write_text(program(0));argv=[a.cli,'check',str(source),'--resident','--json','--resident-reset-after','16','--resident-capacity','16']
p=subprocess.Popen(argv,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
hello=json.loads(p.stdout.readline());assert hello['ready'];rows=[]
try:
    for i in range(80):
        source.write_text(program(i));start=time.perf_counter();p.stdin.write('compile\n');p.stdin.flush();r=json.loads(p.stdout.readline());r['request_wall_ms']=(time.perf_counter()-start)*1000
        assert r['report']['status']=='Success' and r['report']['verification_runs']==1,r
        assert r['active_queries']==0 and r['cached_queries']<=16,r
        assert r['session_cleared']==((i+1)%16==0),r
        if r['session_cleared']:assert r['cached_queries']==0,r
        if i%16 in (0,15):
            fresh=subprocess.run([a.cli,'check',str(source),'--json'],capture_output=True,text=True,timeout=60)
            f=json.loads(fresh.stdout);assert fresh.returncode==0 and f['verification_runs']==1
            for key in ('status','diagnostics','scalar_interfaces','input_fingerprint'):
                assert r['report'][key]==f[key],(i,key,r['report'][key],f[key])
        rows.append(r)
    p.stdin.write('clear\nstats\n');p.stdin.flush();clear=json.loads(p.stdout.readline());stats=json.loads(p.stdout.readline());assert clear['cleared'] and stats['cached_queries']==0 and stats['retained_requests']==0
    p.stdin.write('quit\n');p.stdin.flush();assert p.wait(timeout=10)==0
finally:
    if p.poll() is None:p.kill();p.wait()
    stderr=p.stderr.read();(a.out/'stderr').write_text(stderr)
    (a.out/'rows.json').write_text(json.dumps(rows,indent=2)+'\n')
    summary={'complete':len(rows)==80,'requests':len(rows),'resets':sum(r['session_cleared'] for r in rows),'fresh_oracle_checks':10 if len(rows)==80 else None,'rss_mib_by_epoch_end':[r['rss_kib']/1024 for r in rows if r['session_cleared']],'max_rss_mib':max((r['rss_kib']/1024 for r in rows),default=0),'byte_limit_claimed':False,'stderr':stderr}
    (a.out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary))

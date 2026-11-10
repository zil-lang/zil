from pathlib import Path
p=Path('.task/resident/worker.rs');s=p.read_text();s=s.replace('FunctionMerging::After }','FunctionMerging::AfterOptimization }');p.write_text(s)
p=Path('crates/zil-driver/examples/resident_session.rs');p.parent.mkdir(exist_ok=True);p.write_text(s)
p=Path('.task/resident/bench/measure.py');s=p.read_text()
s=s.replace("'e69091059ed999a5279303cdd07e9f602703f079'", "'3e4a2a2ae368380e86ea99da32d7f3eef68cf3b4'")
s=s.replace("arms=['base','cand','rust'] if mode=='O2' else ['base','cand']", "arms=['base','cand']")
s=s.replace("arms=['base','cand','rust'];random.Random", "arms=['base','cand'];random.Random")
assert "arms=['base','cand','rust']" not in s
needle="    log.mkdir(parents=True)\n    inp=log/'stdin'"
assert s.count(needle)==1
s=s.replace(needle,"    if len(argv)>1 and argv[1]=='--resident-request':return resident_timed(argv,log,timeout)\n"+needle)
needle="        argv=[tools[arm],'check' if mode=='check' else 'build',source,'--json','--semantic-cache',work/'semantic']"
assert s.count(needle)==1
s=s.replace(needle,"        if arm=='cand':return [tools[arm],'--resident-request',mode,source,exe,work,'32']\n"+needle)
needle="                    if not (block==0 and mode=='O2'):shutil.rmtree"
assert s.count(needle)==1;s=s.replace(needle,"                    close_workers()\n"+needle)
s=s.replace("    finally:\n        dump(out/'failures.json'", "    finally:\n        close_workers()\n        dump(out/'failures.json'",1)
s=s.replace("'boundary':'fresh process through link; project caches retained only within one history; OS caches retained; no Cargo Fresh in paired rows'", "'boundary':'base fresh CLI vs candidate resident protocol including IPC and cold startup; current Verify each request; project caches/history/input bytes held equal; no Cargo Fresh; capacity 32 raw query entries is not a byte budget'")
s=s.replace("'upstream_original_crates':'separate unpaired lane, excluded from paired aggregate; no substitution claimed'", "'rust_reference':'NotRun: this experiment isolates resident memory from process/disk reuse, not compiler backend or language'")
injected='''
# Serialize all requests to one worker per cache history; never retain an
# unbounded population of project workers across cases or rounds.
WORKERS={}
def resident_timed(argv,log,timeout):
    import select
    log.mkdir(parents=True)
    _,_,mode,source,exe,work,capacity=map(str,argv)
    start=time.perf_counter_ns();key=work
    def read_line(p):
        if not select.select([p.stdout],[],[],timeout)[0]:
            os.killpg(p.pid,signal.SIGKILL);p.wait();raise TimeoutError('resident response deadline')
        line=p.stdout.readline()
        if not line:raise RuntimeError('resident worker exited before response')
        return json.loads(line)
    started=False
    if key not in WORKERS:
        err=open(str(Path(work)/'resident.stderr'),'wb')
        p=subprocess.Popen([str(argv[0]),mode,source,exe,work,capacity],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,start_new_session=True,bufsize=0)
        WORKERS[key]=(p,err);hello=read_line(p);assert hello['ready'] and hello['protocol']==1;started=True
    p,err=WORKERS[key]
    p.stdin.write(b'compile\\n');p.stdin.flush();answer=read_line(p)
    elapsed=(time.perf_counter_ns()-start)/1e6
    report=answer['report'];stdout=(json.dumps(report)+'\\n').encode()
    (log/'stdout').write_bytes(stdout);(log/'stderr').write_bytes(b'');(log/'stdin').write_bytes(b'compile\\n')
    (log/'resident.json').write_text(json.dumps(answer,indent=2))
    row={'argv':list(map(str,argv)),'wall_ms':elapsed,'cpu_ms':None,'rss_kib':answer['rss_kib'],'peak_kib':answer['peak_kib'],'exit':0 if report['status']=='Success' else 1,'timeout':False,'log':str(log),'resident_pid':p.pid,'worker_start_included':started,'cached_queries':answer['cached_queries'],'active_queries':answer['active_queries'],'inner_ms':answer['elapsed_ms']}
    assert answer['cached_queries']<=int(capacity),answer
    return row,stdout,b''
def close_workers():
    for p,err in WORKERS.values():
        try:
            p.stdin.write(b'quit\\n');p.stdin.flush();p.wait(timeout=10)
            if p.returncode!=0:raise RuntimeError('resident worker exit '+str(p.returncode))
        finally:
            if p.poll() is None:os.killpg(p.pid,signal.SIGKILL);p.wait()
            p.stdin.close();p.stdout.close();err.close()
    WORKERS.clear()
'''
s=s.replace('def main():',injected+'\ndef main():',1)
p.write_text(s)
print('Prepared persistent worker and exact original eight-case protocol; no production semantic changes')

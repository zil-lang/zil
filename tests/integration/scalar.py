#!/usr/bin/env python3
"""Real scalar-source regression suite. Requires a built ZIL CLI and LLVM."""
import argparse,datetime,hashlib,json,os,pathlib,subprocess,sys,uuid
ROOT=pathlib.Path(__file__).resolve().parents[2]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def save(p,d):p.write_text(json.dumps(d,ensure_ascii=False,indent=2)+'\n')
def call(command,folder):
    folder.mkdir(parents=True)
    p=subprocess.run([str(v) for v in command],capture_output=True,text=True,timeout=60,cwd=ROOT)
    (folder/'stdout.txt').write_text(p.stdout);(folder/'stderr.txt').write_text(p.stderr)
    save(folder/'command.json',{'command':[str(v) for v in command],'exit_code':p.returncode})
    return p
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--out-root',default='out/runs/scalar');a=parser.parse_args()
    binary=pathlib.Path(os.environ.get('ZIL_BIN',ROOT/'target/debug/zil')).resolve()
    if not binary.is_file():raise RuntimeError('build zil-cli before executing this suite')
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+uuid.uuid4().hex[:8]
    out=ROOT/a.out_root/stamp;out.mkdir(parents=True,exist_ok=False)
    manifest=ROOT/'tests/integration/scalar-cases.json';cases=json.loads(manifest.read_text())['cases'];results=[]
    for c in cases:
        folder=out/c['id'];folder.mkdir();src=ROOT/c['source'];exe=folder/'program'
        r={'id':c['id'],'status':'Fail','source':c['source'],'source_sha256':sha(src),'expected':c,'assertions':0,'evidence_level':'NativeSource' if c['kind']!='reject' else 'SourceRejection'}
        try:
            cmd=[binary,'build',src,'--json','--output',exe,'--emit-dir',folder/'stages']
            if 'entry' in c:cmd+=['--entry',c['entry']]
            p=call(cmd,folder/'compile');report=json.loads(p.stdout);r['compile']=report
            if c['kind']=='reject':
                assert p.returncode!=0 and report['status']==c['category'],report
                assert any(d['phase']==c['phase'] and d['code']==c['code'] for d in report['diagnostics']),report
                assert not exe.exists(),'rejection published executable'
                r['assertions']=3
            else:
                assert p.returncode==0 and report['status']=='Success',report
                assert exe.is_file(),'successful compile produced no executable'
                p=call([exe],folder/'run');r['actual_exit_code']=p.returncode;r['actual_stdout']=p.stdout
                if c['kind']=='trap':assert p.returncode==-c['signal'],(p.returncode,p.stderr)
                elif 'entry' in c:assert p.returncode==0 and int(p.stdout.strip())==c['return_value'],(p.returncode,p.stdout,p.stderr)
                else:assert p.returncode==c['exit_code'],(p.returncode,p.stdout,p.stderr)
                r['assertions']=3
                r['machine_sha256']=sha(exe);r['llvm_sha256']=sha(pathlib.Path(report['artifacts']['llvm']))
            r['status']='Pass'
        except (OSError,ValueError,KeyError,AssertionError,subprocess.TimeoutExpired) as ex:r['reason']=str(ex)
        save(folder/'result.json',r);results.append(r)
    try:commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True,stderr=subprocess.DEVNULL).strip()
    except subprocess.CalledProcessError:commit=None
    report={'schema_version':1,'run_id':stamp,'candidate_commit':commit,'scope':'Scalar source integration; not resource/closure/async/full-language acceptance','selected':len(cases),'executed':len(results),'passed':sum(c['status']=='Pass' for c in results),'assertions':sum(c['assertions'] for c in results),'compiler_sha256':sha(binary),'manifest_sha256':sha(manifest),'status':'Pass' if results and all(c['status']=='Pass' for c in results) else 'Fail','cases':results}
    save(out/'result.json',report)
    print(json.dumps({**{k:v for k,v in report.items() if k!='cases'},'result_path':str((out/'result.json').relative_to(ROOT))},indent=2))
    return report['status']!='Pass'
if __name__=='__main__':sys.exit(main())

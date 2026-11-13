#!/usr/bin/env python3
"""Fresh V25 evidence restore; candidate runtime replay additionally needs fixed V23 SDK18."""
import argparse,hashlib,subprocess,sys,tempfile,urllib.request
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__,epilog='SDK18 complete prefix: f0a9bf9ec100653e1736ca2db1d2bd19f3266d94/research/incremental-demand-full-push-20261112-v23-04/V23-INDEX.json. Candidate compiler999ced requires SDK18 DSO785a, not baseline/main DSO2bf. This command restores evidence; it does not install or execute the SDK.')
p.add_argument('--work-dir',type=Path,required=True);p.add_argument('--restore-root',type=Path);p.add_argument('--receipt',type=Path,required=True);a=p.parse_args()
print('Candidate replay dependency: fixed V23 SDK18 prefix, DSO785a; compiled7bac compiler999ced. Prior mainAff complete custody remains V24. No compiler/native execution by this restore.',flush=True)
url='https://raw.githubusercontent.com/zil-lang/zil/4b7cb33ada7e7491609f8354add0599036a53a95/research/incremental-demand-late-stage-20261113-v25-index/restore_all_https.py'
with urllib.request.urlopen(url,timeout=60) as r:body=r.read()
assert len(body)==6531 and hashlib.sha256(body).hexdigest()=='478e4c53e4f9cd16e6e92f25a1d6f8323540a9bcf823273ce23bd301335edcd8'
with tempfile.TemporaryDirectory(prefix='v25-fixed-restorer-') as tmp:
 q=Path(tmp)/'restore_all_https.py';q.write_bytes(body)
 cmd=[sys.executable,str(q),'4b7cb33ada7e7491609f8354add0599036a53a95','--work-dir',str(a.work_dir),'--receipt',str(a.receipt)]
 if a.restore_root:cmd+=['--restore-root',str(a.restore_root)]
 sys.exit(subprocess.run(cmd).returncode)

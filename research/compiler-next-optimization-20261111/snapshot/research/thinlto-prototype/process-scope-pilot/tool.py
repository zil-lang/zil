import subprocess,sys,os,json,time
from pathlib import Path
if sys.argv[2]=="normal":
 Path(sys.argv[1]).write_text(json.dumps({"tool":os.getpid(),"group":os.getpgrp()}));raise SystemExit(0)
p=subprocess.Popen(["sleep","30"])
Path(sys.argv[1]).write_text(json.dumps({"tool":os.getpid(),"grandchild":p.pid,"group":os.getpgrp()}))
p.wait()

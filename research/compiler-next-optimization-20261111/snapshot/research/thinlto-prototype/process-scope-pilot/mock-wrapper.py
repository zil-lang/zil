import sys
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from processes import measured
command=[sys.executable,sys.argv[2],sys.argv[3],sys.argv[4]]
r=measured(command,Path(sys.argv[5]),float(sys.argv[6]),own_group=False)
raise SystemExit(0 if r["exit"]==0 else 1)

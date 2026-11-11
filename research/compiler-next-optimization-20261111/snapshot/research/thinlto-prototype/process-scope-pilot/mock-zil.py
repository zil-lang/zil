import subprocess,sys
p=subprocess.Popen([sys.executable,sys.argv[1],*sys.argv[2:]])
raise SystemExit(p.wait())

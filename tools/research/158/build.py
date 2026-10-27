import pathlib,subprocess,os,hashlib,json
r=pathlib.Path(__import__('os').environ.get('ZIL_WORK_ROOT','/mnt/data/zil-round154')); tool=r/'rust/bin/rustc'; source=r/'wt158/tools/research/158/session_driver.rs'; manifest={}
for label,target in [('A','target-base'),('C','target-158')]:
 d=r/target/'release/deps'; cmd=[str(tool),'--edition=2024','-C','opt-level=3','-C','panic=abort',str(source),'-L',f'dependency={d}']
 for name in ['zil_driver','zil_base']:
  libs=list(d.glob(f'lib{name}-*.rlib'));assert len(libs)==1,(name,libs);cmd+=['--extern',f'{name}={libs[0]}']
 out=r/f'driver158-{label}';cmd+=['-o',str(out)];subprocess.run(cmd,check=True);manifest[label]={'binary':str(out),'sha256':hashlib.sha256(out.read_bytes()).hexdigest(),'command':cmd}
(r/'evidence/driver158-binaries.json').write_text(json.dumps(manifest,indent=2));print(json.dumps(manifest,indent=2))

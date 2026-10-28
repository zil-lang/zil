#!/usr/bin/env python3
"""Export exact tracked source using the established 169 exclusion policy."""
import argparse, hashlib, importlib.util, json, subprocess, zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
def main():
    p=argparse.ArgumentParser();p.add_argument('--out',type=Path,required=True);a=p.parse_args()
    def git(*args):return subprocess.check_output(['git','-C',str(ROOT),*args],text=True).strip()
    if git('status','--porcelain'):raise SystemExit('Commit all source before packaging')
    spec=importlib.util.spec_from_file_location('source169',ROOT/'tools/language/169/package.py')
    m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
    commit=git('rev-parse','HEAD');source,excluded=m.source_inventory(ROOT,commit)
    manifest={'schema':'zil-source-212/1','commit':commit,'crates_tree':git('rev-parse','HEAD:crates'),
      'base':'b8e34416c2a1673a4be8955739cf4f10133fec1e','files':{},'excluded':excluded}
    a.out.parent.mkdir(parents=True,exist_ok=True)
    with zipfile.ZipFile(a.out,'x') as z:
        for name,(data,mode) in sorted(source.items()):
            m.put(z,'zil/'+name,data,mode)
            manifest['files'][name]={'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'mode':mode}
        m.put(z,'SOURCE-MANIFEST-212.json',(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n').encode())
    print(json.dumps({'source_commit':commit,'files':len(source),'bytes':a.out.stat().st_size,'sha256':hashlib.sha256(a.out.read_bytes()).hexdigest()}))
if __name__=='__main__':main()

#!/usr/bin/env python3
"""Restore local worktrees only. No network/GitHub writes and no overwrite."""
import argparse,pathlib,subprocess,json,hashlib,re
p=argparse.ArgumentParser();p.add_argument('--bundle',required=True);p.add_argument('--manifest',required=True);p.add_argument('--destination',required=True);a=p.parse_args();bundle=pathlib.Path(a.bundle).resolve();manifest=json.loads(pathlib.Path(a.manifest).read_text());dst=pathlib.Path(a.destination).resolve()
assert not dst.exists(),f'refusing to overwrite {dst}'
assert hashlib.sha256(bundle.read_bytes()).hexdigest()==manifest['bundle']['sha256'],'bundle SHA256 mismatch'
dst.mkdir(parents=True);bare=dst/'repository.git';subprocess.run(['git','clone','--mirror',str(bundle),str(bare)],check=True);subprocess.run(['git','--git-dir',str(bare),'fsck','--full'],check=True)
for name,ref in manifest['worktrees'].items():
 assert pathlib.PurePath(name).name==name and name not in ['.','..'] and re.fullmatch('[0-9a-f]{40}',ref),(name,ref)
 subprocess.run(['git','--git-dir',str(bare),'worktree','add','--detach',str(dst/name),ref],check=True)
 actual=subprocess.check_output(['git','-C',str(dst/name),'rev-parse','HEAD'],text=True).strip();assert actual==ref,(name,actual,ref)
refs=dict(line.split(' ',1)[::-1]for line in subprocess.check_output(['git','--git-dir',str(bare),'for-each-ref','--format=%(objectname) %(refname)'],text=True).splitlines())
for ref,sha in manifest['refs'].items():assert refs.get(ref)==sha,(ref,refs.get(ref),sha)
print(json.dumps({'status':'Pass','destination':str(dst),'worktrees':manifest['worktrees'],'toolchain':'Install/use pinned Rust1.99.1 separately; build caches are deliberately absent.'},indent=2))

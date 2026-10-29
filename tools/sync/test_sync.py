#!/usr/bin/env python3
"""Execute the real PowerShell script against isolated local Git remotes."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile

ROOT = Path(__file__).resolve().parent
SCRIPT = ROOT / 'SYNC-ZIL-v6.ps1'
PWSH = Path(os.environ.get('ZIL_TEST_PWSH') or shutil.which('pwsh') or ROOT/'powershell/pwsh')
ENV = dict(os.environ, XDG_CACHE_HOME=str(ROOT/'runtime-cache'),
           XDG_CONFIG_HOME=str(ROOT/'runtime-config'), XDG_DATA_HOME=str(ROOT/'runtime-data'),
           GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull)
RUN = Path(tempfile.mkdtemp(prefix='e2e-', dir=ROOT))
RESULTS = []

def command(args, cwd=None, check=True, input=None):
    p = subprocess.run([str(x) for x in args], cwd=cwd, env=ENV, input=input,
                       text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
    if check and p.returncode:
        raise AssertionError(f'{args}: exit={p.returncode}\n{p.stdout}')
    return p

def git(repo, *args):
    return command(['git', '-C', repo, *args]).stdout.strip()

def setup(name):
    path = RUN/name
    path.mkdir()
    source = path/'source'
    command(['git', 'init', '-q', '-b', 'main', source])
    git(source, 'config', 'user.name', 'Offline sync test')
    git(source, 'config', 'user.email', 'sync-test@example.invalid')
    commit(source, 'seed.txt', 'initial')
    remote = path/'remote with spaces.git'
    command(['git', 'clone', '-q', '--bare', source, remote])
    return path, source, remote

def commit(repo, file, content):
    (repo/file).write_text(content+'\n')
    git(repo, 'add', file)
    git(repo, 'commit', '-q', '-m', content)
    return git(repo, 'rev-parse', 'HEAD')

def bundle(repo, path, *refs):
    git(repo, 'bundle', 'create', path, *refs)
    return path

def heads(remote):
    return git(remote, 'for-each-ref', '--format=%(refname):%(objectname)', 'refs/heads')

def sync(path, remote, *args, inputs='', script=SCRIPT, expected=0):
    p = command([PWSH, '-NoLogo', '-NoProfile', '-File', script, '-Repo', remote,
                 '-CacheDir', path/'cache with spaces', *args], cwd=path, input=inputs, check=False)
    (path/'last-run.log').write_text(p.stdout)
    assert p.returncode == expected, f'expected {expected}, got {p.returncode}:\n{p.stdout}'
    return p.stdout

def case(name, fn):
    start = time.monotonic()
    try:
        fn(*setup(name))
    except Exception as exc:
        RESULTS.append(dict(name=name, result='FAIL', detail=str(exc), seconds=time.monotonic()-start))
        raise
    else:
        RESULTS.append(dict(name=name, result='PASS', seconds=round(time.monotonic()-start, 3)))
        print('PASS', name, flush=True)

def default_main(path, source, remote):
    sha=commit(source,'main.txt','main update')
    bundle(source,path/'auto.bundle','main')
    out=sync(path,remote,inputs='\n')
    assert '自动发现 Bundle' in out and 'UPDATED' in out and '同步成功' in out
    assert git(remote,'rev-parse','main')==sha
    assert 'ALREADY' in sync(path,remote,'-NonInteractive')

def contained(path, source, remote):
    bundle(source,path/'older.bundle','main')
    sha=commit(source,'main.txt','newer remote')
    git(source,'push',str(remote),'main')
    assert 'CONTAINED' in sync(path,remote,'-NonInteractive')
    assert git(remote,'rev-parse','main')==sha

def divergence(path, source, remote):
    base=git(source,'rev-parse','main')
    commit(source,'left.txt','bundle side')
    bundle(source,path/'diverged.bundle','main')
    git(source,'checkout','-q','-b','other',base)
    remote_sha=commit(source,'right.txt','remote side')
    git(source,'push',str(remote),'other:main')
    out=sync(path,remote,'-NonInteractive',expected=2)
    assert 'DIVERGED' in out and '同步未全部成功' in out
    assert git(remote,'rev-parse','main')==remote_sha

def dryrun(path, source, remote):
    before=heads(remote)
    commit(source,'main.txt','preview')
    bundle(source,path/'preview.bundle','main')
    assert 'WOULD-UPDATE' in sync(path,remote,'-DryRun','-NonInteractive')
    assert heads(remote)==before

def main_only_contained(path, source, remote):
    git(source,'branch','task/already-contained')
    sha=commit(source,'main.txt','integrated main')
    bundle(source,path/'integrated.bundle','main','task/already-contained')
    out=sync(path,remote,'-MainOnly','-NonInteractive')
    assert '已确认 Bundle 全部分支均被 main 包含' in out
    assert git(remote,'rev-parse','main')==sha
    assert heads(remote).count('\n')==0

def main_only_unmerged(path, source, remote):
    before=heads(remote)
    git(source,'checkout','-q','-b','task/not-merged')
    commit(source,'task.txt','independent task')
    bundle(source,path/'unmerged.bundle','main','task/not-merged')
    out=sync(path,remote,'-MainOnly','-NonInteractive',expected=1)
    assert 'main 尚未包含' in out and 'task/not-merged' in out
    assert heads(remote)==before

def main_only_missing(path, source, remote):
    git(source,'branch','task/only')
    bundle(source,path/'task.bundle','task/only')
    before=heads(remote)
    assert '要求 Bundle 含' in sync(path,remote,'-MainOnly','-NonInteractive',expected=1)
    assert heads(remote)==before

def multibranch_default(path, source, remote):
    main=commit(source,'main.txt','main change')
    git(source,'checkout','-q','-b','task/new')
    task=commit(source,'task.txt','task change')
    bundle(source,path/'both.bundle','main','task/new')
    sync(path,remote,inputs='\n\n')
    assert git(remote,'rev-parse','main')==main
    assert git(remote,'rev-parse','task/new')==task

def rename(path, source, remote):
    main=commit(source,'main.txt','main change')
    git(source,'checkout','-q','-b','task/old')
    task=commit(source,'task.txt','task change')
    bundle(source,path/'both.bundle','main','task/old')
    sync(path,remote,inputs='\ntask/renamed\n')
    assert git(remote,'rev-parse','main')==main
    assert git(remote,'rev-parse','task/renamed')==task
    assert 'refs/heads/task/old:' not in heads(remote)

def skip_main(path, source, remote):
    initial=git(remote,'rev-parse','main')
    commit(source,'main.txt','main change')
    git(source,'checkout','-q','-b','task/new')
    task=commit(source,'task.txt','task change')
    bundle(source,path/'both.bundle','main','task/new')
    sync(path,remote,'-SkipMain','-NonInteractive')
    assert git(remote,'rev-parse','main')==initial
    assert git(remote,'rev-parse','task/new')==task

def allow_main_compat(path, source, remote):
    sha=commit(source,'main.txt','main change')
    bundle(source,path/'main.bundle','main')
    sync(path,remote,'-AllowMain','-NonInteractive')
    assert git(remote,'rev-parse','main')==sha

def duplicate_targets(path, source, remote):
    before=heads(remote)
    commit(source,'main.txt','main change')
    git(source,'branch','task/another')
    bundle(source,path/'both.bundle','main','task/another')
    assert '映射到同一目标' in sync(path,remote,inputs='main\nmain\n',expected=1)
    assert heads(remote)==before

def multiple_files(path, source, remote):
    old=bundle(source,path/'old.bundle','main')
    sha=commit(source,'main.txt','latest change')
    new=bundle(source,path/'new.bundle','main')
    os.utime(old,(100,100));os.utime(new,(200,200))
    out=sync(path,remote,inputs='\n\n')
    assert '找到 2 个候选包' in out and 'new.bundle' in out
    assert git(remote,'rev-parse','main')==sha
    assert '请用 -Bundle 指定' in sync(path,remote,'-NonInteractive',expected=1)

def zip_auto(path, source, remote):
    sha=commit(source,'main.txt','zip change')
    b=bundle(source,source/'packed.bundle','main')
    with zipfile.ZipFile(path/'package.zip','w') as z:
        z.write(b,'nested/package.bundle')
        z.writestr('../unrelated.txt','must never extract')
    out=sync(path,remote,'-NonInteractive')
    assert '从 ZIP 解出 Bundle' in out
    assert git(remote,'rev-parse','main')==sha
    assert not (path.parent/'unrelated.txt').exists()

def script_dir_fallback(path, source, remote):
    sha=commit(source,'main.txt','fallback change')
    folder=path/'delivery';folder.mkdir()
    script=folder/SCRIPT.name
    shutil.copyfile(SCRIPT,script)
    bundle(source,folder/'fallback.bundle','main')
    assert '自动发现 Bundle' in sync(path,remote,'-NonInteractive',script=script)
    assert git(remote,'rev-parse','main')==sha

def incremental(path, source, remote):
    base=git(source,'rev-parse','main')
    sha=commit(source,'main.txt','incremental change')
    bundle(source,path/'incremental.bundle','main','^'+base)
    sync(path,remote,'-MainOnly','-NonInteractive')
    assert git(remote,'rev-parse','main')==sha

def server_reject(path, source, remote):
    before=heads(remote)
    commit(source,'main.txt','rejected change')
    bundle(source,path/'main.bundle','main')
    hook=remote/'hooks/pre-receive'
    hook.write_text('#!/bin/sh\necho rejected-by-test >&2\nexit 1\n');hook.chmod(0o755)
    assert 'PUSH-FAILED' in sync(path,remote,'-NonInteractive',expected=2)
    assert heads(remote)==before

def skip_interactive(path, source, remote):
    before=heads(remote)
    commit(source,'main.txt','skip change')
    bundle(source,path/'main.bundle','main')
    assert '全部跳过' in sync(path,remote,inputs='-\n')
    assert heads(remote)==before

CASES=[default_main,contained,divergence,dryrun,main_only_contained,main_only_unmerged,
       main_only_missing,multibranch_default,rename,skip_main,allow_main_compat,
       duplicate_targets,multiple_files,zip_auto,script_dir_fallback,incremental,
       server_reject,skip_interactive]
try:
    for fn in CASES: case(fn.__name__,fn)
finally:
    report=dict(script=str(SCRIPT),sha256=hashlib.sha256(SCRIPT.read_bytes()).hexdigest(),
                runtime=command([PWSH,'-NoLogo','-NoProfile','-Command','$PSVersionTable.PSVersion.ToString()']).stdout.strip(),
                platform=sys.platform, run_directory=str(RUN), tests=RESULTS)
    (ROOT/'test-results.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print(f'{len(RESULTS)} cases passed; {RUN}')

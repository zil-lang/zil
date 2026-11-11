#!/usr/bin/env python3
"""Owner-authorized compiler branch retirement. No source/main rewrite.
Archive parents retain full source/evidence histories before any ref is removed.
Changed refs, active jobs and unexpected open PRs are preserved, never guessed.
"""
from pathlib import Path
import datetime, hashlib, json, os, subprocess, zipfile
ROOT=Path.cwd(); TEMP=Path(os.environ['RUNNER_TEMP']); REPO='zil-lang/zil'
OUT=TEMP/'retirement'; OUT.mkdir(exist_ok=True)

def run(*args, cwd=ROOT, check=True, input=None, env=None):
    return subprocess.run(args,cwd=cwd,check=check,capture_output=True,text=True,input=input,env=env)
def api(path, method='GET', data=None):
    args=['gh','api',f'repos/{REPO}/'+path]
    if method!='GET':args+=['--method',method]
    if data is not None:args+=['--input','-']
    r=run(*args,input=json.dumps(data) if data is not None else None)
    return json.loads(r.stdout) if r.stdout.strip() else None
def pages(path, key=None):
    result=[]
    for page in range(1,31):
        value=api(path+('&' if '?' in path else '?')+f'per_page=100&page={page}')
        rows=value[key] if key else value
        result+=rows
        if len(rows)<100:return result
    raise RuntimeError('pagination incomplete')
def refs():return {r['name']:r['commit']['sha'] for r in pages('branches')}
def active():
    result={}
    for status in ('in_progress','queued','waiting','requested','pending'):
        for r in pages('actions/runs?status='+status,'workflow_runs'):
            result[r['id']]={k:r.get(k) for k in ('id','head_branch','head_sha','status','name')}
    return list(result.values())
def contains(a,b):return run('git','merge-base','--is-ancestor',a,b,check=False).returncode==0

def archive_commit(entries, completed=False):
    # Re-read on contention; no reset/clean, no branch switch and no forced update.
    for attempt in range(3):
        run('git','fetch','--filter=blob:none','--no-tags','origin','refs/heads/archive/source-history')
        old=run('git','rev-parse','FETCH_HEAD').stdout.strip()
        manifest=json.loads(run('git','show',old+':manifest.json').stdout)
        parents=[old]
        if not completed:
            heads=list(dict.fromkeys(e['sha'] for e in entries))
            parents += [h for h in heads if not contains(h,old) and not any(h!=p and contains(h,p) for p in heads)]
        old_remote=dict(manifest.get('remote_refs',{}))
        manifest['prior_archive']=old
        manifest['created_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
        manifest['commit_parents']=parents
        manifest['commit_tips']=list(dict.fromkeys(manifest.get('commit_tips',[])+[e['sha'] for e in entries]))
        manifest.setdefault('remote_refs',{}).update({'refs/heads/'+e['branch']:e['sha'] for e in entries})
        retired=manifest.setdefault('retirements',[])
        for e in entries:
            ref='refs/heads/'+e['branch'];old_entry=next((r for r in retired if r.get('ref')==ref and r.get('head_before_retirement')==e['sha']),None)
            record={'ref':ref,'head_before_retirement':e['sha'],'retirement_completed':completed,'reason':e['reason'],'disposition':e['category'],'evidence_refs':e['evidence'],'archive_is_acceptance':False}
            if old_entry is None:retired.append(record)
            else:old_entry.update(record)
        manifest['scope']='Preserve the existing archive plus exact inactive compiler source and evidence histories. Retired names are not lost commits; rejected experiments are not accepted source.'
        payload=json.dumps(manifest,ensure_ascii=False,indent=2)+'\n'
        index=TEMP/f'archive-index-{os.getpid()}-{attempt}-{int(completed)}'
        env=os.environ|{'GIT_INDEX_FILE':str(index),'GIT_AUTHOR_NAME':'github-actions[bot]','GIT_AUTHOR_EMAIL':'41898282+github-actions[bot]@users.noreply.github.com','GIT_COMMITTER_NAME':'github-actions[bot]','GIT_COMMITTER_EMAIL':'41898282+github-actions[bot]@users.noreply.github.com'}
        run('git','read-tree',old,env=env)
        blob=run('git','hash-object','-w','--stdin',input=payload,env=env).stdout.strip()
        run('git','update-index','--add','--cacheinfo','100644',blob,'manifest.json',env=env)
        tree=run('git','write-tree',env=env).stdout.strip()
        args=['git','commit-tree',tree]
        for parent in parents:args+=['-p',parent]
        new=run(*args,input=('Record completed compiler ref retirements' if completed else 'Preserve audited compiler source tips before retirement')+'\n',env=env).stdout.strip()
        for e in entries:assert contains(e['sha'],new),(e['branch'],'archive reachability')
        result=run('git','push','origin',new+':refs/heads/archive/source-history',check=False)
        if result.returncode==0:
            remote=api('git/ref/heads/archive/source-history')['object']['sha'];assert remote==new
            git_remote=run('git','ls-remote','origin','refs/heads/archive/source-history').stdout.split()[0];assert git_remote==new
            return new
    raise RuntimeError('archive moved repeatedly; no ref was forced')

# The audited artifact includes hidden .task evidence inside this ZIP.
run('gh','run','download','42936007346','-R',REPO,'-n','compiler-consolidation-evidence-42936007346','-D',str(TEMP/'audit-input'))
with zipfile.ZipFile(TEMP/'audit-input/audit-with-task-evidence.zip') as z:
    audit=json.loads(z.read('audit.json'))
assert audit['refs']['main']=='2bb30ce1819137d666c073299aa2639da4fd2964'
reasons={
 'task/G10.5.1.3':('delivered','Signature dependency facets are already delivered in PR70; this ref retains the experiment and measurements.', ['PR70','.task/G10.5.1.3/RESULTS.md']),
 'task/G10.5.1.4':('rejected','Both buffer-growth outlining variants failed the original generated-runtime guard.', ['PR72','.task/G10.5.1.4/OUTLINING-RESULTS.md']),
 'task/G10.5.1.5':('delivered','Exact resource-order representation is delivered by PR75; no ongoing work remains on the experiment ref.', ['PR75','42766117593']),
 'task/G10.5.1.6':('delivered','Cleanup suffix sharing is delivered by PR76; preserve original failures and successful evidence.', ['PR76','.task/G10.5.1.6/RESULTS.md']),
 'task/G10.5.1.7':('rejected','Out-of-line destructor boundary regressed inactive/mostly-empty workloads; PR77 is not accepted.', ['PR77','42800092569']),
 'task/G10.5.1.8':('rejected','Package empty-aware refinement still regressed an original runtime case by18.3%.', ['42814450893','.task/G10.5.1.8/RESULTS.md']),
 'task/byte-buffer-growth-boundary':('rejected','Production extraction of rejected growth-boundary candidate; no runtime acceptance despite passing functional CI.', ['PR72','42756277204']),
 'task/closure-destructor-boundary':('rejected','Production extraction of rejected destructor boundary, superseded by explicit negative quality conclusion.', ['PR77','42800092569']),
 'task/closure-helper-partition':('superseded','Historical helper-partition source and unsuccessful predecessors are carried by the current production/composition candidate; no separate implementation stream needed.', ['PR80','PR84','.task/helper-partition/RESULTS.md']),
 'task/compiler-closure-dependencies':('delivered','Anonymous closure linkage is delivered by PR79; retain the bounded performance findings and outstanding gaps.', ['PR79','.task/closure-dependencies/RESULTS.md']),
 'task/compiler-inline-equivalence':('superseded','Expanded-pipeline screen is superseded by the fixed, maintainable normalized candidate; probe selector must not enter main.', ['42928304327','PR84']),
 'task/compiler-inline-prefix':('superseded','Short-prefix screening has completed and is absorbed into the normalized fixed-policy candidate; preserve its negative variants.', ['42931006690','PR84']),
 'task/compiler-merge-order':('superseded','Raw early merge and pass-position probes are finished; rejected runtime outcomes and conclusions are preserved, production continues in PR84.', ['42924878932','42927870902','PR84']),
 'task/compiler-resident-cli':('delivered','Resident compilation is delivered by PR78; benchmark-only branch is not an active implementation.', ['PR78','.task/resident-cli/RESULTS.md']),
 'task/compiler-resident-session':('superseded','The resident-session prototype is superseded by the delivered resident CLI and explicit memory-lifetime controls.', ['PR78','.task/resident/RESULTS.md']),
 'task/destructor-runtime-neighbors':('evidence-only','Completed counterexample validation; the evidence remains necessary but does not require a live source branch.', ['42800092569']),
 'task/helper-current-main-validation':('evidence-only','Completed exact-current-main validation is now carried by PR80/PR84 composition; preserve all weighted negative/neutral observations.', ['42927538037','PR80','PR84']),
 'task/helper-public-boundary':('evidence-only','Completed public-signature boundary validation; retained as immutable evidence for the continuing candidate.', ['42863910118','PR80']),
 'task/normalized-merging-consumers':('evidence-only','Completed unchanged-source handler and State consumer controls; continue under the combined candidate rather than a separate consumer branch.', ['42933848863','PR84']),
 'task/repeatable-closure-drop':('rejected','Static byte cleanup violates original runtime guard for16 closures (10.9%-11.2% regression); no new production acceptance.', ['PR83','42930165591']),
 'task/resident-memory-validation':('evidence-only','Completed retention/memory validation supports already delivered explicit resident mode; no unmerged production feature.', ['PR78','.task/resident/MEMORY-RESULTS.md']),
 'task/static-byte-closure-cleanup':('rejected','Frozen production extraction of rejected static cleanup; keep exact source and tests in archive, close PR83.', ['PR83','42930165591']),
}
allowed_pr={72:'task/byte-buffer-growth-boundary',77:'task/closure-destructor-boundary',83:'task/static-byte-closure-cleanup'}
state={'audit_run':42936007346,'requested_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'retired':[],'skipped':[],'main_modified':False,'goal_updated':False}
(OUT/'receipt.json').write_text(json.dumps(state,indent=2)+'\n')
run('git','config','remote.origin.promisor','true');run('git','config','remote.origin.partialclonefilter','blob:none')
if run('git','rev-parse','--is-shallow-repository').stdout.strip()=='true':run('git','fetch','--unshallow','--filter=blob:none','--no-tags','origin','+refs/heads/*:refs/remotes/origin/*')
else:run('git','fetch','--filter=blob:none','--no-tags','origin','+refs/heads/*:refs/remotes/origin/*')
current=refs();running=active();open_prs=pages('pulls?state=open');eligible=[]
for name,(category,reason,evidence) in reasons.items():
    expected=audit['refs'][name]
    if current.get(name)!=expected:
        state['skipped'].append({'branch':name,'reason':'changed or missing since audit','expected':expected,'actual':current.get(name)});continue
    if any(r['head_branch']==name for r in running):
        state['skipped'].append({'branch':name,'reason':'active workflow; retained'});continue
    linked=[p for p in open_prs if p['head']['ref']==name]
    if any(allowed_pr.get(p['number'])!=name for p in linked):
        state['skipped'].append({'branch':name,'reason':'unexpected open PR; retained'});continue
    run('git','cat-file','-e',expected+'^{tree}')
    eligible.append({'branch':name,'sha':expected,'category':category,'reason':reason,'evidence':evidence,'open_prs':[p['number'] for p in linked]})
assert eligible,'No audited branches eligible for retirement'
state['refs_before']=current;state['active_before']=running;state['archive_before']=current['archive/source-history']
state['archive_preservation']=archive_commit(eligible)
(OUT/'receipt.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n')
# All destructive changes occur only after verified remote archive publication.
fresh=refs();running=active();deleting=[]
for e in eligible:
    name=e['branch']
    if fresh.get(name)!=e['sha'] or any(r['head_branch']==name for r in running):
        state['skipped'].append({'branch':name,'reason':'changed or became active after archival; retained'});continue
    for number in e['open_prs']:
        pr=api('pulls/'+str(number));assert pr['head']['ref']==name and pr['head']['sha']==e['sha']
        if pr['state']=='open':
            api(f'issues/{number}/comments','POST',{'body':f"按用户本轮专项收束要求退役此已确认未采用的候选。{e['reason']}\n\n精确head `{e['sha']}` 已作为可达父提交保全到固定 `archive/source-history@{state['archive_preservation']}`；原源码、测试、正负测量均保留，归档不代表验收。此处关闭PR并删除无效工作入口，不将候选合入main。当前仍在验收的PR80/PR84及其活跃任务不受影响。"})
            api('pulls/'+str(number),'PATCH',{'state':'closed'})
            assert api('pulls/'+str(number))['state']=='closed'
    deleting.append(e)
if deleting:
    args=['git','push','--atomic','origin']
    for e in deleting:args+=['--force-with-lease=refs/heads/'+e['branch']+':'+e['sha']]
    args += [':refs/heads/'+e['branch'] for e in deleting]
    # Leases guard deletions against concurrent writers. No history is force-updated.
    run(*args)
    after=refs()
    for e in deleting:assert e['branch'] not in after
    state['retired']=deleting;state['refs_after']=after
    state['archive_final']=archive_commit(deleting,completed=True)
else:state['archive_final']=state['archive_preservation'];state['refs_after']=refs()
state['completed_at']=datetime.datetime.now(datetime.timezone.utc).isoformat()
(OUT/'receipt.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n')
# Goal is a separate guarded status ref, never merged into main.
for attempt in range(3):
    run('git','fetch','--filter=blob:none','--no-tags','origin','refs/heads/goal');before=run('git','rev-parse','FETCH_HEAD').stdout.strip()
    work=TEMP/f'consolidation-goal-{attempt}';run('git','worktree','add','--detach',str(work),before)
    f=work/'goal.md';lines=f.read_text().splitlines();out=[]
    for line in lines:
        if line.startswith('| G10.5.1 |'):
            line='| G10.5.1 | 编译效率复核、加权性能与真实覆盖扩展 | 进行中 | P1 | Chat编译专项 | PR65/70/75/76/78/79已合；失效研究入口已固定归档后收束，恢复与未关闭范围见[编译分支收据](records/compiler-branch-consolidation.json)。PR80/PR84及活跃验收保留；不以阶段收益关闭一般增量或Rust90运行父门。 |'
        elif line.startswith('| G10.5.1.4 |'):
            line='| G10.5.1.4 | 拥有闭包扩容边界与运行质量 | 未采用·已归档 | — | — | 原两候选越过运行退化门，PR72关闭；源码、失败及全部测量在固定归档中保全，不进入main。 |'
        elif line.startswith('| G10.5.1.7 |'):
            line='| G10.5.1.7 | 大量拥有环境析构边界 | 未采用·已归档 | — | — | 原空/少活跃环境运行退化10.6%–35.6%，PR77关闭；原始正负结果完整保留，不能用功能CI替代运行验收。 |'
        elif line.startswith('| G10.5.1.8 |'):
            line='| G10.5.1.8 | Package空环境内联析构入口 | 未采用·已归档 | — | — | 32闭包低负载运行仍慢18.3%；相关研究入口退役，原代码/清理故障与质量证据仍可恢复。 |'
        elif line.startswith('| G10.5.1.11 |'):
            line='| G10.5.1.11 | 资源闭包端到端细粒度增量与后端分区 | 候选组合验收中 | P1 | Chat编译专项 · PR80/84 | 多模块helper分区与规范化等价管线由当前组合候选承接；旧研究/筛选入口归档。静态闭包清理PR83因真实运行退化未采用。以精确合流后的完整质量与频率结果裁决，不累加不同窗口百分比。 |'
        out.append(line)
    f.write_text('\n'.join(out)+'\n');(work/'records').mkdir(exist_ok=True)
    (work/'records/compiler-branch-consolidation.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n')
    run('git','add','goal.md','records/compiler-branch-consolidation.json',cwd=work)
    run('git','-c','user.name=github-actions[bot]','-c','user.email=41898282+github-actions[bot]@users.noreply.github.com','commit','-m','Record compiler branch consolidation and preserve active candidate boundaries',cwd=work)
    r=run('git','push','origin','HEAD:refs/heads/goal',cwd=work,check=False)
    if r.returncode==0:
        state['goal_before']=before;state['goal_commit']=run('git','rev-parse','HEAD',cwd=work).stdout.strip();state['goal_updated']=True;break
else:raise RuntimeError('Goal repeatedly changed; archive and deletion receipts remain preserved')
run('git','fetch','--filter=blob:none','--no-tags','origin','refs/heads/goal');state['goal_readback']=run('git','rev-parse','FETCH_HEAD').stdout.strip();assert contains(state['goal_commit'],state['goal_readback'])
saved=json.loads(run('git','show',state['goal_readback']+':records/compiler-branch-consolidation.json').stdout);assert saved['archive_final']==state['archive_final'] and saved['retired']==state['retired']
state['refs_final']=refs();(OUT/'receipt.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n')
d=ROOT/'.task/compiler-consolidation';d.mkdir(parents=True,exist_ok=True);(d/'retirement.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n')
run('git','fetch','--no-tags','origin','refs/heads/task/compiler-consolidation');assert run('git','rev-parse','FETCH_HEAD').stdout.strip()==os.environ['GITHUB_SHA']
run('git','add','.task/compiler-consolidation/retirement.json')
run('git','-c','user.name=github-actions[bot]','-c','user.email=41898282+github-actions[bot]@users.noreply.github.com','commit','-m','Preserve verified archive, retirement and goal readback receipt')
run('git','push','origin','HEAD:refs/heads/task/compiler-consolidation')
print(json.dumps({'retired':len(state['retired']),'skipped':state['skipped'],'archive':state['archive_final'],'goal':state['goal_readback'],'remaining':state['refs_final']},ensure_ascii=False,indent=2))

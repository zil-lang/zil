#!/usr/bin/env python3
from pathlib import Path
# Reuse the reviewed guarded archive helpers, not the earlier retirement body.
exec(Path('.task/compiler-consolidation/retire.py').read_text().split('# The audited artifact includes')[0], globals())
import re,posixpath
prior=json.loads((ROOT/'.task/compiler-consolidation/retirement.json').read_text())
state=dict(prior);state['late_evidence_review']=[]
run('git','config','remote.origin.promisor','true');run('git','config','remote.origin.partialclonefilter','blob:none')
if run('git','rev-parse','--is-shallow-repository').stdout.strip()=='true':run('git','fetch','--unshallow','--filter=blob:none','--no-tags','origin','+refs/heads/*:refs/remotes/origin/*')
else:run('git','fetch','--filter=blob:none','--no-tags','origin','+refs/heads/*:refs/remotes/origin/*')
late=[
 ('task/helper-current-main-validation','835364edf2a7741587829543177d9b25dffc7a81','467b5ffd791c6aae80cc524b1c23cf7259182661','evidence-only','Final immutable report added after first audit; implementation unchanged, candidate continues in PR80/84.',['42927538037','.task/helper-current/RESULTS.md']),
 ('task/repeatable-closure-drop','a5d1f1d57dacbe14ead435be0d39f17609c4a79e','71d36d72fcdaacc8d73ccdf1eb9ac23d217d7071','rejected','Final negative-quality report added after first audit; static cleanup remains rejected, PR83 closed.',['42930165591','PR83']),
]
current=refs();running=active();extra=[]
for name,old,head,category,reason,evidence in late:
    if current.get(name)!=head or any(r['head_branch']==name for r in running):
        state['late_evidence_review'].append({'branch':name,'status':'Retained: changed or active','actual':current.get(name)});continue
    assert contains(old,head)
    changed=run('git','diff','--name-only',old,head).stdout.splitlines()
    assert changed and all(p.startswith('.task/') and p.endswith('.md') for p in changed),(name,changed)
    extra.append({'branch':name,'sha':head,'category':category,'reason':reason,'evidence':evidence,'open_prs':[]})
    state['late_evidence_review'].append({'branch':name,'old':old,'head':head,'changed_paths':changed,'production_unchanged':True})
if extra:
    archive=archive_commit(extra)
    fresh=refs();running=active()
    assert all(fresh.get(e['branch'])==e['sha'] and not any(r['head_branch']==e['branch'] for r in running) for e in extra)
    args=['git','push','--atomic','origin']+['--force-with-lease=refs/heads/'+e['branch']+':'+e['sha'] for e in extra]+[':refs/heads/'+e['branch'] for e in extra]
    run(*args);after=refs();assert all(e['branch'] not in after for e in extra)
    state['retired']+=extra;state['archive_final']=archive_commit(extra,True)
state['skipped']=[r for r in state['skipped'] if r['branch'] not in {e['branch'] for e in state['retired']}]
assert len({e['branch'] for e in state['retired']})==len(state['retired'])
(OUT/'final-receipt.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n')

# Owner authorized integrating lasting decisions. This is a documentation-only
# fast-forward on current main; no pending compiler candidate enters the tree.
text=(ROOT/'.task/compiler-consolidation/compiler-cost-contract.md').read_text()
path='docs/guide/compiler-cost-contract.md'
assert text.startswith('# ') and text.endswith('\n') and '.task/' not in text
for attempt in range(3):
    run('git','fetch','--filter=blob:none','--no-tags','origin','refs/heads/main');base=run('git','rev-parse','FETCH_HEAD').stdout.strip()
    exists=run('git','cat-file','-e',base+':'+path,check=False).returncode==0
    if exists:
        assert run('git','show',base+':'+path).stdout==text,'Existing document differs; no overwrite'
        state['decision_commit']=base;state['decision_already_present']=True;break
    build=run('git','show',base+':docs/guide/build.md').stdout
    needle='## 回归和复现\n';assert build.count(needle)==1
    build=build.replace(needle,'## 优化采用边界\n\n缓存与分区粒度、生成代码质量、完整请求加权及对照失败合同遵守[编译优化的成本与采用边界](compiler-cost-contract.md)。研究结论不自动改变默认策略或放宽当前验证。\n\n'+needle)
    for link in re.findall(r'\]\(([^)]+)\)',text):
        assert '://' not in link and not link.startswith('/'),link
        target=posixpath.normpath(posixpath.join('docs/guide',link.split('#')[0]));assert target.startswith('docs/guide/')
        run('git','cat-file','-e',base+':'+target)
    index=TEMP/f'decision-index-{attempt}'
    env=os.environ|{'GIT_INDEX_FILE':str(index),'GIT_AUTHOR_NAME':'github-actions[bot]','GIT_AUTHOR_EMAIL':'41898282+github-actions[bot]@users.noreply.github.com','GIT_COMMITTER_NAME':'github-actions[bot]','GIT_COMMITTER_EMAIL':'41898282+github-actions[bot]@users.noreply.github.com'}
    run('git','read-tree',base,env=env)
    for target,value in [(path,text),('docs/guide/build.md',build)]:
        blob=run('git','hash-object','-w','--stdin',input=value,env=env).stdout.strip();run('git','update-index','--add','--cacheinfo','100644',blob,target,env=env)
    tree=run('git','write-tree',env=env).stdout.strip()
    commit=run('git','commit-tree',tree,'-p',base,input='docs: preserve compiler cost, identity and runtime acceptance boundaries\n\nDocumentation-only consolidation under explicit owner authorization. Preserve current compiler and tests byte-for-byte; no rejected or pending optimization is adopted. Validate exact two-path diff, whitespace and every relative link. Historical raw evidence stays in archive/source-history rather than main.\n',env=env).stdout.strip()
    changed=run('git','diff','--name-only',base,commit).stdout.splitlines();assert sorted(changed)==sorted([path,'docs/guide/build.md'])
    run('git','diff','--check',base,commit)
    result=run('git','push','origin',commit+':refs/heads/main',check=False)
    if result.returncode==0:
        assert api('git/ref/heads/main')['object']['sha']==commit
        state['decision_base']=base;state['decision_commit']=commit;state['decision_tree']=tree;state['decision_changed_paths']=changed;state['decision_checks']=['exact documentation-only diff','all source/test/toolchain files unchanged','git diff --check','all relative documentation links resolve'];break
else:raise RuntimeError('Main changed repeatedly; no forced update')

# Persist this round before retiring this task's own coordination-only ref.
state['retired_compiler_count']=len(state['retired']);state['finalized_at']=datetime.datetime.now(datetime.timezone.utc).isoformat()
f=ROOT/'.task/compiler-consolidation/final-receipt.json';f.write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n')
run('git','fetch','--no-tags','origin','refs/heads/task/compiler-consolidation');assert run('git','rev-parse','FETCH_HEAD').stdout.strip()==os.environ['GITHUB_SHA']
run('git','add','.task/compiler-consolidation/final-receipt.json')
run('git','-c','user.name=github-actions[bot]','-c','user.email=41898282+github-actions[bot]@users.noreply.github.com','commit','-m','Record late evidence retirement and documentation-only main integration')
head=run('git','rev-parse','HEAD').stdout.strip();run('git','push','origin','HEAD:refs/heads/task/compiler-consolidation')
self_entry={'branch':'task/compiler-consolidation','sha':head,'category':'completed-coordination','reason':'Branch audit and guarded retirement complete; no compiler implementation pending on this temporary branch.','evidence':['.task/compiler-consolidation/final-receipt.json'],'open_prs':[]}
archive_commit([self_entry]);running=active();assert not any(r['head_branch']==self_entry['branch'] and r['id']!=int(os.environ['GITHUB_RUN_ID']) for r in running)
assert refs().get(self_entry['branch'])==head
run('git','push','origin','--force-with-lease=refs/heads/'+self_entry['branch']+':'+head,':refs/heads/'+self_entry['branch'])
state['coordination_branch_retired']=self_entry;state['archive_final']=archive_commit([self_entry],True)
for e in state['retired']+[self_entry]:assert contains(e['sha'],state['archive_final'])
state['refs_final']=refs();state['active_final']=active()
assert all(e['branch'] not in state['refs_final'] for e in state['retired']+[self_entry])
# Update only the receipt and already-owned task summary, preserving all other
# authors' current statuses, source work and task rows.
for attempt in range(3):
    run('git','fetch','--filter=blob:none','--no-tags','origin','refs/heads/goal');old=run('git','rev-parse','FETCH_HEAD').stdout.strip()
    work=TEMP/f'final-goal-{attempt}';run('git','worktree','add','--detach',str(work),old)
    (work/'records/compiler-branch-consolidation.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n')
    f=work/'goal.md';s=f.read_text();needle='[编译分支收据](records/compiler-branch-consolidation.json)'
    if needle in s:
        s=s.replace(needle,needle+'；稳定采用边界已归入main的`docs/guide/compiler-cost-contract.md`',1);f.write_text(s)
    run('git','add','goal.md','records/compiler-branch-consolidation.json',cwd=work)
    run('git','-c','user.name=github-actions[bot]','-c','user.email=41898282+github-actions[bot]@users.noreply.github.com','commit','-m','Finalize compiler branch retirement and lasting decision links',cwd=work)
    r=run('git','push','origin','HEAD:refs/heads/goal',cwd=work,check=False)
    if r.returncode==0:state['goal_commit']=run('git','rev-parse','HEAD',cwd=work).stdout.strip();break
else:raise RuntimeError('Goal conflict; final artifact retains exact completed operations')
run('git','fetch','--no-tags','origin','refs/heads/goal');state['goal_readback']=run('git','rev-parse','FETCH_HEAD').stdout.strip();assert contains(state['goal_commit'],state['goal_readback'])
saved=json.loads(run('git','show',state['goal_readback']+':records/compiler-branch-consolidation.json').stdout);assert saved['archive_final']==state['archive_final'] and saved['decision_commit']==state['decision_commit']
state['refs_final']=refs();(OUT/'final-receipt.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'retired_compiler_count':state['retired_compiler_count'],'coordination_branch_deleted':True,'archive':state['archive_final'],'decision_commit':state['decision_commit'],'goal':state['goal_readback'],'remaining':state['refs_final']},ensure_ascii=False,indent=2))

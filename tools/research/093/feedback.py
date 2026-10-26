#!/usr/bin/env python3
"""Bind original Lab feedback requests to real schema16 production artifacts."""
import argparse, json, subprocess
from pathlib import Path
from run import BASE, NAMES, normalized, sha, write

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--runs',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True)
    p.add_argument('--repo',type=Path,required=True)
    a=p.parse_args(); runs=a.runs.resolve(); out=a.out.resolve(); repo=a.repo.resolve()
    out.mkdir(parents=True,exist_ok=False)
    pin={}
    for name in ['_zil_design.md','spec/interfaces.json','validation/product-gates.json',
                 'crates/zil-types/src/resource_effects.rs','crates/zil-analyze/src/resources.rs',
                 'crates/zil-verify/src/resources.rs','crates/zil-ir/src/lib.rs',
                 'crates/zil-ir/src/program.rs','crates/zil-driver/src/session.rs',
                 'crates/zil-driver/src/contracts.rs']:
        result=subprocess.run(['git','show',BASE+':'+name],cwd=repo,capture_output=True)
        if result.returncode!=0:
            # The pinned source file inventory is explicit; never substitute a
            # similarly named file if the requested historical file is absent.
            pin[name]=dict(status='AbsentAtBaseline',stderr=result.stderr.decode())
            continue
        path=out/'baseline'/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(result.stdout)
        pin[name]=dict(file=str(path),sha256=sha(path))
    template_path=runs/'lab-replay-001/source/feedback/receipt-template-0.6.0.json'
    template=json.loads(template_path.read_text())
    identity=json.loads((runs/'frozen-002/identity.json').read_text())
    metadata=json.loads((runs/'lab-replay-001/production-cases.json').read_text())
    plain=json.loads((runs/'bridge-B-001/report.json').read_text())
    counted=json.loads((runs/'bridge-B-counted-001/report.json').read_text())
    assert plain['production_commit']==counted['production_commit']==BASE
    assert plain['schema']==counted['schema']==16 and not plain['counting'] and counted['counting']
    projects=json.loads((runs/'project-bridge-001/result.json').read_text())
    project_by={row['case']:row for row in projects}
    bridge_command=json.loads((runs/'bridge-B-counted-001-runner.command.json').read_text())
    rows=[]
    for m,x,y in zip(sorted(metadata,key=lambda x:x['case']),plain['cases'],counted['cases']):
        assert m['case']==x['case']==y['case'] and sha(m['source'])==m['sha256']
        assert normalized(x)==normalized(y),(m['case'],'instrumentation changed observed result')
        report=y['report'];artifacts=report['artifacts']
        artifact_hashes={kind:dict(file=file,sha256=sha(file)) for kind,file in artifacts.items()}
        case=m['case'];project=project_by.get(case)
        exported=project['cold']['artifacts']['verified-interfaces'] if project else None
        receipt=dict(template)
        receipt.update(repository=str(repo),commit=BASE,specVersion='schema16 / ZIL-LANG-1.3',
            caseId=case,sourceFile=m['source'],command=bridge_command['argv'],
            toolchain={k:identity[k] for k in ['rustc','cargo','clang','profile']},
            result=report['status'],stdoutFile=str(runs/'bridge-B-counted-001-runner.stdout'),
            stderrFile=str(runs/'bridge-B-counted-001-runner.stderr'),
            typedCfgFile=artifacts.get('typed'),exportedContractFile=exported,
            phaseMeasurements=[],productGatesFile=pin['validation/product-gates.json']['file'],
            notes=m['relationship']+'; '+m['expected_scope']+
                  '; phase measurements are not available: counts below are observed body visits, not time or CPU instructions',
            phase=report['phase'],diagnostics=report['diagnostics'],sourceSha256=m['sha256'],
            artifactHashes=artifact_hashes,observedCounters=dict(zip(NAMES,y['counts'])),
            verificationRuns=report['verification_runs'],
            scalarInterfaces=project['cold']['scalar_interfaces'] if project else [],
            exportedContractSha256=sha(exported) if exported else None,
            exportedContractScope='compile_path Verify project' if project else 'NotObserved: single-source bridge does not produce project interfaces')
        if project:
            receipt['exportCommand']=json.loads((runs/'project-bridge-001'/f'{case}-runner.command.json').read_text())
        write(out/'receipts'/f'{case}.json',receipt);rows.append(receipt)
    diamond=[dict(case=r['caseId'],depth=int(r['caseId'][7:]),
        types=r['observedCounters']['types_effect_body_visits'],
        analyze=r['observedCounters']['analyze_resource_body_visits'],
        verify=r['observedCounters']['verify_resource_body_visits']) for r in rows if r['caseId'].startswith('diamond')]
    for row in diamond:
        assert row['types']==2*(row['depth']+1)
        assert row['analyze']==row['verify']==2*(row['depth']+2)
    pair=[project_by[name]['cold']['scalar_interfaces'][0] for name in ['recursive-write','recursive-before-write']]
    assert pair[0]['signature_fingerprint']==pair[1]['signature_fingerprint']
    assert pair[0]['contract_fingerprint']==pair[1]['contract_fingerprint']
    assert pair[0]['implementation_fingerprint']!=pair[1]['implementation_fingerprint']
    write(out/'result.json',dict(status='Pass',baseline=BASE,schema=16,baseline_files=pin,
        template_sha256=sha(template_path),bridge_cases=len(rows),project_requests=sum(len(x['records']) for x in
            [json.loads((runs/'project-bridge-001'/name/'report.json').read_text()) for name in project_by]),
        successful_lower_cases=sum(r['result']=='Success' for r in rows),
        rejected_cases=[dict(case=r['caseId'],phase=r['phase'],diagnostics=r['diagnostics']) for r in rows if r['result']!='Success'],
        diamond_body_visits=diamond,ordered_contract_counterexample=pair,
        assertions=['Plain/counted status, diagnostic, query, artifact hashes identical on all 18 cases',
                    'All input bytes equal retained generation metadata',
                    'Observed diamond body visits fit the stated linear counts at all 8 measured depths',
                    'Final exported contract hash equal for distinct ordinary prefix/return behavior examples'],
        limitations=['No machine execution of recursion or diamond runtime',
            'Desc.destroy, Desc.drop and recursive by-value Node rejected by current production features',
            'Equal contract hashes do not establish unsafe caching: implementation fingerprints differ and remain dependencies',
            'These counts prove behavior only on these source families, not arbitrary programs or a universal complexity bound']))

if __name__=='__main__':main()

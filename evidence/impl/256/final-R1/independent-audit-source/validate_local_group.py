#!/usr/bin/env python3
"""Independently validate restored consumer groups from one finalized local full run."""
import argparse,hashlib,json,re,stat,sys
from pathlib import Path
from audit_run import frozen_consumer_sources,need,sha,write_json
from consumer256 import verify as core_verify
from consumer256_supplement import verify as supplement_verify
from select_restore import selected

def audit(a,result):
    index=json.loads(a.index.read_text())
    restoration=json.loads((a.index.parent/'RESTORE.json').read_text())
    need(restoration['status']=='Pass' and restoration['all_content_addressed_blobs']=='Pass',
         'No successful complete-blob restoration receipt')
    need(restoration['script_sha256']==sha(Path(__file__).with_name('select_restore.py')),
         'Restoration script identity differs')
    need(restoration['expected_head']==a.expected_head==index['checkout'], 'Exact source identity differs')
    need(restoration['archive_sha256']==sha(a.archive), 'Finalized archive identity differs')
    entries=[e for e in index['files'] if selected(e['path'])]
    for e in entries:
        p=a.runner/e['path'];s=p.stat()
        need(s.st_size==e['bytes'] and stat.S_IMODE(s.st_mode)==e['mode'] and sha(p)==e['sha256'],
             'Selected original mode/size/SHA mismatch: '+e['path'])
    need(len(entries)==restoration['selected_files'], 'Selected full file set differs')
    check=json.loads((a.runner/'receipts/check.json').read_text())
    log=a.runner/'receipts/output.log'
    need(check['source_commit']==a.expected_head and check['name']==a.phase
         and check['status']=='Pass' and check['exit_code']==0 and not check['timeout'],
         'Phase receipt failed or has wrong source/phase')
    need(check['log_sha256']==sha(log),'Original full-run log SHA differs')
    summaries=re.findall(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;',log.read_text())
    need(summaries and all(row[0]=='ok' for row in summaries),'Missing/failed original test summaries')
    totals={name:sum(int(row[i+1]) for row in summaries) for i,name in enumerate(['passed','failed','ignored','measured','filtered'])}
    need(totals==check['totals'] and len(summaries)==check['test_summaries'], 'Full-run counts differ from original log')
    need(totals['failed']==totals['ignored']==0,'Full-run failures or ignored tests')
    sources,oracle=frozen_consumer_sources(a.repo,a.expected_head)
    read=lambda name:(a.runner/name).read_bytes()
    core=core_verify(index['files'],read,oracle,sources,'native-evidence/',{'backend':a.backend})
    supplement=supplement_verify(index['files'],read,json.loads(sources['tests/impl256/supplement/ORACLE.json']),sources,'native-evidence/',{'backend':a.backend})
    result.update(status='Pass',implementation_commit=a.expected_head,phase=a.phase,backend=a.backend,
                  archive_sha256=sha(a.archive),archive_bytes=a.archive.stat().st_size,
                  full_index_files=len(index['files']),restored_selected_files=len(entries),
                  selected_logical_bytes=sum(e['bytes'] for e in entries),
                  raw_file_modes_sizes_sha256='Pass',original_json_rewritten=False,
                  full_run_receipt=check,independently_counted_log_totals=totals,
                  core_and_pair=core,supplement=supplement,
                  groups_combined=False,default_and_reference_combined=False,
                  boundary_contract_semantics='Not separately replayed by this native validator; complete source test log verified; pair negative is independently checked.',
                  missing_archive_roots_outside_native_group_scope=index.get('missing_roots'),
                  scope='One exact local full-run native group evidence audit; no new compiler/native execution and no remote CI/full-delivery acceptance claim.')

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['runner','index','archive','repo','output']:p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--expected-head',required=True);p.add_argument('--phase',required=True)
    p.add_argument('--backend',choices=['llvm-api','clang'],required=True)
    a=p.parse_args();need(not a.output.exists(),'Refusing to overwrite audit result')
    result={'status':'Running','reviewer':'/root/assess_delivery_process','script_sha256':sha(Path(__file__)),
            'validators':{name:sha(Path(__file__).with_name(name)) for name in ['consumer256.py','consumer256_supplement.py']}}
    try:audit(a,result)
    except Exception as exc:result.update(status='Fail',error=str(exc),error_type=type(exc).__name__)
    write_json(a.output,result)
    print(json.dumps({'status':result['status'],'output':str(a.output),'error':result.get('error')}))
    return 0 if result['status']=='Pass' else 1
if __name__=='__main__':sys.exit(main())

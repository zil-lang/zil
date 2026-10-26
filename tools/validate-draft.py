#!/usr/bin/env python3
"""Run actual draft checks and preserve both passing and incomplete evidence.

The draft gate covers implemented modules, aggregates, generics, CT, callbacks, shared/exclusive references, finite resources, complete owning packages, synchronous Copy errors with cleanup scopes, and bounded byte-buffer C-ABI native I/O. It never promotes a partial
resource/closure conformance report to full-language or product acceptance.
"""
import argparse,datetime,hashlib,json,os,pathlib,re,shutil,subprocess,sys,time,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1]
def save(path,data):path.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def publish_summary(root,report):
    """Explicitly publish a receipt; never mutate the original raw result."""
    if not re.fullmatch(r'[A-Za-z0-9_-]+',report['run_id']):raise ValueError('invalid summary RunId')
    folder=root/'evidence/validation'/report['run_id'];folder.mkdir(parents=True,exist_ok=False)
    published={**report,'raw_result_path':report['result_path'],'result_path':str((folder/'result.json').relative_to(root))}
    with (folder/'result.json').open('x') as handle:handle.write(json.dumps(published,ensure_ascii=False,indent=2)+'\n')
    save(root/'evidence/final-summary.json',published)
    return published
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--out-root',default='out/runs/validation');p.add_argument('--publish-summary',action='store_true',help='explicitly publish a compact evidence receipt after the run')
    p.add_argument('--win64-imports',default=os.environ.get('ZIL_WIN64_IMPORTS'),help='explicit directory of Win64 import archives')
    p.add_argument('--wine',default=os.environ.get('ZIL_WINE'),help='explicit Wine64 compatibility executor; requires configured isolated X11 environment')
    p.add_argument('--byte-heap-library',default=os.environ.get('ZIL_BYTE_HEAP_LIBRARY'),help='explicit Win64 Heap* import archive for sealed byte storage')
    a=p.parse_args()
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+uuid.uuid4().hex[:8]
    output_root=(ROOT/a.out_root).resolve()
    if a.publish_summary and output_root==(ROOT/'evidence/validation').resolve():p.error('--publish-summary requires a raw output root distinct from evidence/validation')
    if not output_root.is_relative_to(ROOT):raise ValueError('output root escapes project')
    out=output_root/stamp;out.mkdir(parents=True,exist_ok=False);records=[]
    def run(name,command,required=True,timeout=240,expected_status=None,executed_field=None,minimum_executed=1):
        folder=out/name;folder.mkdir();started=time.monotonic()
        record={'name':name,'command':command,'required_for_scalar_draft':required,'exit_code':None}
        try:
            proc=subprocess.run(command,cwd=ROOT,text=True,capture_output=True,timeout=timeout,env={**os.environ,'ZIL_TEST_REPORT_DIR':str(folder),'ZIL_BIN':str(ROOT/'target/debug/zil')})
            stdout,stderr=proc.stdout,proc.stderr;record['exit_code']=proc.returncode
        except (OSError,subprocess.TimeoutExpired) as ex:
            stdout='';stderr=str(ex);record['error']=str(ex)
        (folder/'stdout.txt').write_text(stdout);(folder/'stderr.txt').write_text(stderr)
        record['seconds']=time.monotonic()-started;record['passed']=record['exit_code']==0
        record['stdout_path']=str((folder/'stdout.txt').relative_to(ROOT));record['stderr_path']=str((folder/'stderr.txt').relative_to(ROOT))
        try:record['reported']=json.loads(stdout)
        except ValueError:pass
        if expected_status is not None:
            reported=record.get('reported',{})
            structured=isinstance(reported,dict) and reported.get('status')==expected_status
            record['expected_status']=expected_status
            if executed_field is not None:
                count=reported.get(executed_field) if isinstance(reported,dict) else None
                structured=structured and type(count) is int and count>=minimum_executed
                record['minimum_execution']={executed_field:minimum_executed}
            record['passed']=record['passed'] and structured
            if not structured:record['result_error']='Missing, nonpassing or zero/incomplete structured result'
        if name in ('rust-tests','native-roam-tests'):record['rust_test_functions_passed']=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',stdout)))
        if name=='native-roam-tests':
            record['minimum_test_functions']=36
            required_native={
                'independent_model_anchors_cover_width_traps_wrapping_and_lazy_evaluation',
                'typed_generation_is_deterministic_and_invalid_input_never_becomes_expected',
                'actual_native_matches_independent_pins_and_generated_programs',
                'native_fault_is_observable_and_identity_budget_invalid_statuses_remain_distinct',
                'native_timeout_and_output_quota_reclaim_owned_descendants_only',
                'scalar_native_queue_replays_reduces_and_closes_with_mixed_profile_regressions',
                'host::tests::snapshot_uses_open_handle_when_source_path_is_replaced',
                'independent_lifecycle_model_covers_capture_call_and_cleanup_order',
                'generation_and_shrinking_keep_the_model_contract',
                'supervised_native_lifecycles_and_controlled_failure_replay',
                'lifecycle_finding_uses_the_existing_queue_replay_and_shrink_pipeline',
                'byte_storage_model_pins_capacity_failure_and_initialized_bytes',
                'byte_storage_native_c_and_model_agree_at_both_optimizations',
                'byte_storage_finding_replays_reduces_rejects_bad_candidate_and_closes',
            }
            observed_native=set(re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$',stdout,re.M))
            record['required_native_tests']=sorted(required_native)
            record['missing_native_tests']=sorted(required_native-observed_native)
            record['passed']=record['passed'] and record['rust_test_functions_passed']>=36 and not record['missing_native_tests']
        save(folder/'command.json',record);records.append(record)
        print(json.dumps({'check':name,'exit_code':record['exit_code'],'required_for_scalar_draft':required}),flush=True)
        return stdout
    rust=run('rust-version',['rustc','--version','--verbose'])
    clang=run('llvm-version',[os.environ.get('ZIL_CLANG','clang'),'--version'])
    run('build',['cargo','build','--workspace','--offline','--locked','--target-dir',str(ROOT/'target')])
    built_compiler=ROOT/'target/debug/zil'
    built_compiler_sha=sha(built_compiler) if records[-1]['passed'] and built_compiler.is_file() else None
    # The provenance-sensitive state campaign still requires committed clean
    # sources; output placement does not weaken that requirement.
    source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    records[-1]['compiler_provenance']={'source_commit':source_commit,'compiler_path':str(built_compiler.resolve()),'compiler_sha256':built_compiler_sha}
    save(out/'build'/'command.json',records[-1])
    roam_manifest='tools/roam-rs/Cargo.toml'
    run('native-roam-build',['cargo','build','--locked','--offline','--manifest-path',roam_manifest,'--bin','roam'])
    campaign=out/'state-paths'/'campaign'
    run('state-paths',[str(ROOT/'tools/roam-rs/target/debug/roam'),'campaign','--out='+str(campaign),'--commit='+source_commit],timeout=300,expected_status='pass',executed_field='normal_paths',minimum_executed=84)
    from native_roam_gate import read_check
    state_record=records[-1];state_record['campaign_validation']=read_check(campaign/'report.json',source_commit)
    state_record['passed']=state_record['passed'] and state_record['campaign_validation']['status']=='Pass'
    if not state_record['passed']:state_record['result_error']='Native campaign failed execution or source/count/property checks'
    save(out/'state-paths'/'command.json',state_record)
    run('native-roam-report-controls',[sys.executable,'tests/native_roam_gate.test.py','--report',str(campaign/'report.json'),'--commit',source_commit],expected_status='Pass',executed_field='executed',minimum_executed=25)
    run('native-roam-format',['cargo','fmt','--manifest-path',roam_manifest,'--','--check'])
    run('native-roam-clippy',['cargo','clippy','--locked','--offline','--manifest-path',roam_manifest,'--all-targets','--','-D','warnings'])
    run('native-roam-tests',['cargo','test','--locked','--offline','--manifest-path',roam_manifest])
    run('format',['cargo','fmt','--all','--','--check'])
    run('clippy',['cargo','clippy','--workspace','--all-targets','--offline','--locked','--','-D','warnings'])
    # Keep collecting independent test targets after a failure; the exit status
    # still fails the gate, while one old fixture cannot hide later regressions.
    run('rust-tests',['cargo','test','--workspace','--offline','--locked','--no-fail-fast'])
    run('scalar-source',[sys.executable,'tests/integration/scalar.py','--out-root',str(out/'scalar-source'/'campaign')])
    run('native-stdio-rust-comparison',[sys.executable,'tests/integration/impl044.py','--zil',str(built_compiler),'--out',str(out/'native-stdio-rust-comparison'/'campaign'),'--evidence',str(out/'native-stdio-rust-comparison'/'receipt.json')],expected_status='Pass',executed_field='checks',minimum_executed=32)
    run('closure-source-oracle',[sys.executable,'tests/integration/closures.py','--compiler',str(built_compiler),'--seed','23041','--cases','24','--out-root',str(out/'closure-source-oracle'/'campaign')],timeout=300,expected_status='Pass',executed_field='executed',minimum_executed=26)
    closure_record=records[-1]
    closure_report=closure_record.get('reported',{})
    closure_checks=(closure_report.get('source_commit')==source_commit
                    and closure_report.get('source_dirty') is False
                    and closure_report.get('provenance_stable') is True
                    and closure_report.get('provenance_start')==closure_report.get('provenance_end')
                    and closure_report.get('compiler_path')==str(built_compiler.resolve())
                    and built_compiler_sha is not None
                    and closure_report.get('compiler_sha256')==built_compiler_sha==sha(built_compiler)
                    and closure_report.get('harness_sha256')==sha(ROOT/'tests/integration/closures.py')
                    and closure_report.get('normal_cases')==24
                    and closure_report.get('mutation_controls')==2
                    and closure_report.get('native_executions')==52
                    and type(closure_report.get('state_actions')) is int and closure_report['state_actions']>=49
                    and closure_report.get('passed')==26
                    and closure_report.get('assertions')==182)
    closure_record['passed']=closure_record['passed'] and closure_checks
    if not closure_checks:closure_record['result_error']='Closure corpus identity/counts/native executions or mutation controls are incomplete'
    save(out/'closure-source-oracle'/'command.json',closure_record)
    run('borrowed-closure-source-oracle',[sys.executable,'tests/integration/borrowed_closures.py','--compiler',str(built_compiler),'--seed','23042','--cases','24','--out-root',str(out/'borrowed-closure-source-oracle'/'campaign')],timeout=300,expected_status='Pass',executed_field='executed',minimum_executed=30)
    borrow_record=records[-1]
    borrow_report=borrow_record.get('reported',{})
    borrow_checks=(borrow_report.get('source_commit')==source_commit
                   and borrow_report.get('source_dirty') is False
                   and borrow_report.get('provenance_stable') is True
                   and borrow_report.get('provenance_start')==borrow_report.get('provenance_end')
                   and borrow_report.get('compiler_path')==str(built_compiler.resolve())
                   and built_compiler_sha is not None
                   and borrow_report.get('compiler_sha256')==built_compiler_sha==sha(built_compiler)
                   and borrow_report.get('harness_sha256')==sha(ROOT/'tests/integration/borrowed_closures.py')
                   and borrow_report.get('normal_cases')==24
                   and borrow_report.get('mutation_controls')==2
                   and borrow_report.get('rejection_cases')==4
                   and borrow_report.get('native_executions')==52
                   and type(borrow_report.get('model_steps')) is int and borrow_report['model_steps']>=7
                   and borrow_report.get('passed')==30
                   and borrow_report.get('assertions')==276)
    borrow_record['passed']=borrow_record['passed'] and borrow_checks
    if not borrow_checks:borrow_record['result_error']='Shared capture corpus identity, pointer evidence, native counts or rejection/mutation controls are incomplete'
    save(out/'borrowed-closure-source-oracle'/'command.json',borrow_record)
    run('mixed-capture-rust-comparison',[sys.executable,'tests/integration/mixed_captures.py','--compiler',str(built_compiler),'--seed','23043','--out-root',str(out/'mixed-capture-rust-comparison'/'campaign')],timeout=300,expected_status='Pass',executed_field='executed',minimum_executed=13)
    mixed_record=records[-1]
    mixed_report=mixed_record.get('reported',{})
    mixed_checks=(mixed_report.get('source_commit')==source_commit
                  and mixed_report.get('source_dirty') is False
                  and mixed_report.get('provenance_stable') is True
                  and mixed_report.get('provenance_start')==mixed_report.get('provenance_end')
                  and mixed_report.get('compiler_path')==str(built_compiler.resolve())
                  and built_compiler_sha is not None
                  and mixed_report.get('compiler_sha256')==built_compiler_sha==sha(built_compiler)
                  and mixed_report.get('harness_sha256')==sha(ROOT/'tests/integration/mixed_captures.py')
                  and mixed_report.get('helper_sha256')==sha(ROOT/'tests/integration/borrowed_closures.py')
                  and mixed_report.get('normal_cases')==8
                  and mixed_report.get('mutation_controls')==2
                  and mixed_report.get('rejection_cases')==3
                  and mixed_report.get('zil_native_executions')==20
                  and mixed_report.get('rust_native_executions')==10
                  and mixed_report.get('passed')==13
                  and mixed_report.get('assertions')==135)
    mixed_record['passed']=mixed_record['passed'] and mixed_checks
    if not mixed_checks:mixed_record['result_error']='Mixed capture identity, model controls or paired Rust/ZIL execution counts are incomplete'
    save(out/'mixed-capture-rust-comparison'/'command.json',mixed_record)
    run('native-cli',[sys.executable,'tests/script/native-protocol.py'])
    win_campaign=out/'win64-platform-closure'/'campaign'
    if a.win64_imports and a.wine:
        win_command=[sys.executable,'tools/platform/087/verify.py','--compiler',str(built_compiler),'--imports',a.win64_imports,'--wine',a.wine,'--out',str(win_campaign)]
    else:
        win_command=[sys.executable,'-c','import json; print(json.dumps({"status":"InfraError","error":"Win64 closure requires --win64-imports and --wine, or ZIL_WIN64_IMPORTS and ZIL_WINE, plus an isolated X11 environment."})); raise SystemExit(2)']
    run('win64-platform-closure',win_command,timeout=300,expected_status='Pass',executed_field='assertions',minimum_executed=66)
    win_record=records[-1]
    try:
        win_report=json.loads((win_campaign/'result.json').read_text())
        expected=set()
        for level in ['O0','O2']:
            for case in ['fail_create','fail_register','window','abi','stack']:
                for check_name in ['verified complete build','distinct clang/link commands','PE signature','Windows LLVM target']:
                    expected.add(f'{case}-{level}: {check_name}')
            for case in ['fail_create','fail_register']:expected.add(f'{case}-{level}: failure cleanup')
            for check_name in ['one actual X window','real state-dependent paint pixels','actual resize','normal process exit','two input events','exactly one ordered window/class cleanup','all paint resources released','no reported API failure']:
                expected.add(f'window-{level}: {check_name}')
            expected.add(f'abi-{level}: native C ABI agreement')
            expected.add(f'stack-{level}: stack and buffer machine execution')
        expected.add('stack-O0: real __chkstk import')
        expected.add('source and compiler identity stable')
        observed=[item['name'] for item in win_report['records'] if item.get('passed') is True]
        expected_runs={f'{case}-{level}-run':42 if case=='fail_create' else 43 if case=='fail_register' else 0
                       for level in ['O0','O2'] for case in ['fail_create','fail_register','window','abi','stack']}
        actual_runs=[item for item in win_report['commands'] if item.get('stem','').endswith('-run')]
        run_names=[item['stem'] for item in actual_runs]
        machine_commands=(len(actual_runs)==len(expected_runs)
                          and len(run_names)==len(set(run_names))
                          and set(run_names)==set(expected_runs)
                          and all(type(item.get('exit_code')) is int and item['exit_code']==expected_runs[item['stem']]
                                  and item.get('argv')==[str(pathlib.Path(a.wine).resolve()),str(win_campaign/item['stem'][:-4]/'program.exe')]
                                  for item in actual_runs)
                          and all(type(item.get('exit_code')) is int and item['exit_code']==0
                                  for item in win_report['commands'] if item.get('stem') not in expected_runs))
        win_checks=(win_report.get('status')=='Pass'
                    and win_report.get('source_commit')==source_commit
                    and win_report.get('source_dirty') is False
                    and win_report.get('compiler_path')==str(built_compiler.resolve())
                    and built_compiler_sha is not None
                    and win_report.get('compiler_sha256')==built_compiler_sha==sha(built_compiler)
                    and win_report.get('harness_sha256')==sha(ROOT/'tools/platform/087/verify.py')
                    and type(win_report.get('pe_executions')) is int and win_report['pe_executions']==len(actual_runs)==10
                    and machine_commands
                    and win_report.get('execution')=='Win64 PE under Wine on isolated X display; not native Windows'
                    and type(win_report.get('assertions')) is int and type(win_report.get('passed')) is int
                    and win_report['assertions']==win_report['passed']==len(win_report['records'])==len(expected)
                    and all(item.get('passed') is True for item in win_report['records'])
                    and len(observed)==len(set(observed))
                    and expected==set(observed))
        win_record['required_case_assertions']=sorted(expected)
        win_record['missing_case_assertions']=sorted(expected-set(observed))
        win_record['machine_receipt']={'path':str((win_campaign/'result.json').relative_to(ROOT)),'sha256':sha(win_campaign/'result.json'),'pe_executions':win_report.get('pe_executions'),'execution':win_report.get('execution')}
    except (OSError,ValueError,KeyError,TypeError) as ex:
        win_checks=False;win_record['machine_receipt_error']=str(ex)
    win_record['passed']=win_record['passed'] and win_checks
    if not win_checks:win_record['result_error']='Win64 machine source/compiler identity, mandatory cases or actual PE execution counts are incomplete'
    save(out/'win64-platform-closure'/'command.json',win_record)
    slice_campaign=out/'slice-native-abi'/'campaign'
    if a.win64_imports and a.wine:
        slice_clang_name=os.environ.get('ZIL_CLANG','clang')
        slice_clang=pathlib.Path(shutil.which(slice_clang_name) or slice_clang_name).absolute()
        slice_linker=slice_clang.parent/'lld-link'
        slice_command=[sys.executable,'tools/slices/092/verify.py','--compiler',str(built_compiler),'--imports',a.win64_imports,'--wine',a.wine,'--clang',str(slice_clang),'--linker',str(slice_linker),'--out',str(slice_campaign)]
    else:
        slice_command=[sys.executable,'-c','import json; print(json.dumps({"status":"InfraError","error":"Slice ABI requires explicit Win64 imports and Wine."})); raise SystemExit(2)']
    run('slice-native-abi',slice_command,timeout=240,expected_status='Pass',executed_field='executions',minimum_executed=4)
    slice_record=records[-1]
    try:
        slice_report=json.loads((slice_campaign/'result.json').read_text())
        expected_names={f'{target}-{level}' for target in ['native','win64'] for level in ['O0','O2']}
        binaries=slice_report['binaries'];commands=slice_report['commands'];checks=slice_report['checks']
        expected_commands={f'{name}-{step}' for name in expected_names for step in ['build','run']}
        expected_checks={f'tool {path.name}' for path in [built_compiler,pathlib.Path(a.wine),slice_clang,slice_linker]}
        expected_checks.add('explicit kernel32 import archive')
        for name in expected_names:
            expected_checks.update(f'{name}: {check_name}' for check_name in ['complete independent Verify','real tool invocations','binary format','target and actual window representation','no implicit allocator or noalias','quiet successful semantic checks'])
            expected_checks.update([name+'-build: process exit',name+'-run: process exit'])
        slice_checks=(slice_report['status']=='Pass' and slice_report['error'] is None
                      and slice_report['compiler']=={'path':str(built_compiler.absolute()),'sha256':built_compiler_sha}
                      and built_compiler_sha is not None and sha(built_compiler)==built_compiler_sha
                      and slice_report['sources']=={str(path.relative_to(ROOT)):sha(path) for path in sorted((ROOT/'examples/mainline/slices').glob('*.zil'))}
                      and type(slice_report['executions']) is int and slice_report['executions']==len(binaries)==4
                      and {item['name'] for item in binaries}==expected_names
                      and len(commands)==8 and {item['name'] for item in commands}==expected_commands
                      and all(type(item['exit_code']) is int and item['exit_code']==0 for item in commands)
                      and type(slice_report['assertions']) is int and slice_report['assertions']==len(checks)==37
                      and len({item['name'] for item in checks})==37 and {item['name'] for item in checks}==expected_checks
                      and all(item['passed'] is True for item in checks))
        for item in binaries:
            name=item['name'];binary=slice_campaign/name/('program.exe' if name.startswith('win64') else 'program')
            command=next(c for c in commands if c['name']==name+'-run')
            argv=[str(pathlib.Path(a.wine).absolute()),str(binary)] if name.startswith('win64') else [str(binary)]
            target,level=name.split('-')
            build_argv=[str(built_compiler.absolute()),'build',str(ROOT/'examples/mainline/slices/portable.zil'),'--target',target,'--opt',level,'--clang',str(slice_clang),'-o',str(binary),'--emit-dir',str(slice_campaign/name/'phases'),'--json']
            if target=='win64':build_argv+=['--linker',str(slice_linker),'--windows-lib',str(pathlib.Path(a.win64_imports).absolute()/'kernel32.lib')]
            build_command=next(c for c in commands if c['name']==name+'-build')
            slice_checks=(slice_checks and item['sha256']==sha(binary) and item['bytes']==binary.stat().st_size
                          and item['execution']==('Wine compatibility' if name.startswith('win64') else 'Linux native')
                          and build_command['argv']==build_argv
                          and command['argv']==argv and (slice_campaign/(name+'-run.stdout')).read_bytes()==b'')
        slice_record['machine_receipt']={'path':str((slice_campaign/'result.json').relative_to(ROOT)),'sha256':sha(slice_campaign/'result.json'),'executions':slice_report.get('executions'),'scope':'Two Linux native and two Wine compatibility slice executions; no native Windows claim'}
    except (OSError,ValueError,KeyError,TypeError,StopIteration) as ex:
        slice_checks=False;slice_record['machine_receipt_error']=str(ex)
    slice_record['passed']=slice_record['passed'] and slice_checks
    if not slice_checks:slice_record['result_error']='Slice ABI identity, cases, binary hashes or real execution counts are incomplete'
    save(out/'slice-native-abi'/'command.json',slice_record)
    byte_campaign=out/'byte-buffer-native-abi'/'campaign'
    if a.win64_imports and a.wine and a.byte_heap_library:
        byte_clang_name=os.environ.get('ZIL_CLANG','clang')
        byte_clang=pathlib.Path(shutil.which(byte_clang_name) or byte_clang_name).absolute()
        byte_linker=byte_clang.parent/'lld-link'
        byte_command=[sys.executable,'tools/byte-buffer/096/verify.py','--compiler',str(built_compiler),'--imports',a.win64_imports,'--heap-library',a.byte_heap_library,'--wine',a.wine,'--clang',str(byte_clang),'--linker',str(byte_linker),'--out',str(byte_campaign)]
    else:
        byte_command=[sys.executable,'-c','import json; print(json.dumps({"status":"InfraError","error":"Byte storage ABI requires explicit heap imports, kernel32 imports and Wine."})); raise SystemExit(2)']
    run('byte-buffer-native-abi',byte_command,timeout=240,expected_status='Pass',executed_field='executions',minimum_executed=4)
    byte_record=records[-1]
    try:
        byte_report=json.loads((byte_campaign/'result.json').read_text())
        names={f'{target}-{level}' for target in ['native','win64'] for level in ['O0','O2']}
        tools=[built_compiler,pathlib.Path(a.wine).absolute(),byte_clang,byte_linker]
        libraries=[pathlib.Path(a.win64_imports).absolute()/'kernel32.lib',pathlib.Path(a.byte_heap_library).absolute()]
        binaries=byte_report['binaries'];commands=byte_report['commands'];checks=byte_report['checks']
        file_rows=byte_report['files']
        actual_files={str(path.relative_to(byte_campaign)):path for path in byte_campaign.rglob('*') if path.is_file() and path!=byte_campaign/'result.json'}
        if not isinstance(file_rows,list) or len(file_rows)!=len(actual_files):raise ValueError('Byte storage file inventory cardinality differs')
        if len({row['path'] for row in file_rows})!=len(file_rows) or {row['path'] for row in file_rows}!=set(actual_files):raise ValueError('Byte storage file inventory membership differs')
        for row in file_rows:
            path=actual_files[row['path']]
            if path.is_symlink() or not path.resolve().is_relative_to(byte_campaign.resolve()):raise ValueError('Byte storage file escapes campaign')
            if type(row['bytes']) is not int or row['bytes']!=path.stat().st_size or row['sha256']!=sha(path):raise ValueError('Byte storage file content differs: '+row['path'])
        command_names={f'{name}-{step}' for name in names for step in ['build','run']}|{name+'-version' for name in ['compiler','clang','linker','wine']}
        required_logs={name+'.'+stream for name in command_names for stream in ['stdout','stderr']}
        if not required_logs.issubset(actual_files):raise ValueError('Byte storage required command logs missing')
        expected_checks={'tool '+path.name for path in tools}|{'explicit import archive '+path.name for path in libraries}
        expected_checks.update(name+'-version: process exit' for name in ['compiler','clang','linker','wine'])
        for name in names:
            expected_checks.update(name+': '+check_name for check_name in ['full independent Verify','real native tool invocations','executable format','exact target','inline owning head and borrowed window','no unproved noalias','explicit platform allocator','program semantic checks'])
            expected_checks.update([name+'-build: process exit',name+'-run: process exit'])
        byte_checks=(byte_report['status']=='Pass' and byte_report['error'] is None
                     and byte_report['tools']=={str(path.absolute()):sha(path) for path in tools}
                     and built_compiler_sha is not None and sha(built_compiler)==built_compiler_sha
                     and byte_report['inputs']=={str(path):sha(path) for path in libraries}
                     and byte_report['sources']=={str(path.relative_to(ROOT)):sha(path) for path in sorted((ROOT/'examples/mainline/byte_buffer').glob('*.zil'))}
                     and type(byte_report['executions']) is int and byte_report['executions']==len(binaries)==4
                     and {item['name'] for item in binaries}==names
                     and len(commands)==12 and {item['name'] for item in commands}==command_names
                     and all(type(item['exit_code']) is int and item['exit_code']==0 for item in commands)
                     and type(byte_report['assertions']) is int and byte_report['assertions']==len(checks)==50
                     and len({item['name'] for item in checks})==50 and {item['name'] for item in checks}==expected_checks
                     and all(item['passed'] is True for item in checks))
        for name,tool in [('compiler',built_compiler),('clang',byte_clang),('linker',byte_linker),('wine',pathlib.Path(a.wine).absolute())]:
            byte_checks=byte_checks and next(c for c in commands if c['name']==name+'-version')['argv']==[str(tool.absolute()),'--version']
        for item in binaries:
            name=item['name'];target,level=name.split('-');binary=byte_campaign/name/('program.exe' if target=='win64' else 'program')
            argv=[str(pathlib.Path(a.wine).absolute()),str(binary)] if target=='win64' else [str(binary)]
            build_argv=[str(built_compiler.absolute()),'build',str(ROOT/'examples/mainline/byte_buffer/portable.zil'),'--target',target,'--opt',level,'--clang',str(byte_clang),'-o',str(binary),'--emit-dir',str(byte_campaign/name/'phases'),'--json']
            if target=='win64':
                build_argv+=['--linker',str(byte_linker)]
                for library in libraries:build_argv+=['--windows-lib',str(library)]
            build_report=json.loads((byte_campaign/(name+'-build.stdout')).read_text())
            llvm_path=pathlib.Path(build_report['artifacts']['llvm'])
            expected_artifacts={key:str(byte_campaign/name/'phases'/(key+'.txt')) for key in ['candidate','core','ct-core','interfaces','lir','source-map','syntax','typed','verified','verified-interfaces']}
            expected_artifacts.update(executable=str(binary),llvm=str(binary)+'.ll')
            if build_report['artifacts']!=expected_artifacts:raise ValueError('Byte storage full stage artifact identities differ')
            if any(not pathlib.Path(path).is_file() or not pathlib.Path(path).resolve().is_relative_to((byte_campaign/name).resolve()) for path in expected_artifacts.values()):raise ValueError('Byte storage required stage artifact missing or escaped')
            native_commands=build_report['commands']
            if not isinstance(native_commands,list) or len(native_commands)!=(2 if target=='win64' else 1):raise ValueError('Byte storage native command count differs')
            if not all(isinstance(command,list) and all(isinstance(arg,str) for arg in command) for command in native_commands):raise ValueError('Byte storage native command shape differs')
            source_index=6 if target=='win64' else 5
            if len(native_commands[0])<=source_index:raise ValueError('Byte storage clang command truncated')
            staged_llvm=pathlib.Path(native_commands[0][source_index]);staged=staged_llvm.parent
            if (staged_llvm.name!='module.ll' or staged.parent.parent!=(byte_campaign/name)
                or not re.fullmatch(r'\.zil-driver-\d+-\d+',staged.parent.name)
                or not re.fullmatch(r'\.zil-build-\d+-\d+',staged.name)):
                raise ValueError('Byte storage staging command escaped its actual case')
            triple='x86_64-pc-windows-msvc' if target=='win64' else 'x86_64-unknown-linux-gnu'
            clang_argv=[str(byte_clang),'--target='+triple,'-x','ir','-'+level]
            if target=='win64':clang_argv+=['-c']
            clang_argv+=[str(staged_llvm),'-o',str(staged/('module.obj' if target=='win64' else 'program'))]
            expected_native_commands=[clang_argv]
            if target=='win64':expected_native_commands.append([str(byte_linker),'/machine:x64','/subsystem:windows','/entry:zil.pe.entry','/nodefaultlib','/timestamp:0','/out:'+str(staged/'program'),str(staged/'module.obj'),*[str(path) for path in libraries]])
            byte_checks=(byte_checks and item['sha256']==sha(binary) and item['bytes']==binary.stat().st_size
                         and item['execution']==('Wine compatibility' if target=='win64' else 'Linux native')
                         and binary.read_bytes().startswith(b'MZ' if target=='win64' else b'\x7fELF')
                         and llvm_path.resolve().is_relative_to((byte_campaign/name).resolve())
                         and item['llvm_sha256']==sha(llvm_path)
                         and build_report['status']=='Success' and type(build_report['verification_runs']) is int and build_report['verification_runs']==1
                         and native_commands==expected_native_commands and build_report['artifacts']['executable']==str(binary)
                         and next(c for c in commands if c['name']==name+'-build')['argv']==build_argv
                         and next(c for c in commands if c['name']==name+'-run')['argv']==argv
                         and (byte_campaign/(name+'-run.stdout')).read_bytes()==b'')
        byte_record['machine_receipt']={'path':str((byte_campaign/'result.json').relative_to(ROOT)),'sha256':sha(byte_campaign/'result.json'),'executions':byte_report.get('executions'),'scope':'Two Linux native and two Wine compatibility executions of owned byte storage; no native Windows or public aggregate C ABI claim'}
    except (OSError,ValueError,KeyError,TypeError,StopIteration) as ex:
        byte_checks=False;byte_record['machine_receipt_error']=str(ex)
    byte_record['passed']=byte_record['passed'] and byte_checks
    if not byte_checks:byte_record['result_error']='Byte storage identity, cases, hashes or actual machine executions are incomplete'
    save(out/'byte-buffer-native-abi'/'command.json',byte_record)
    run('oracle-model',['node','--experimental-transform-types','--import','./tools/roam/register.mjs','tests/oracle/model-check.mjs'],expected_status='Pass',executed_field='executed',minimum_executed=23)
    run('semantic-oracle',['node','tools/run-oracle.mjs','--seed','14327','--cases','64','--out',str(out/'semantic-oracle'/'campaign')],timeout=300,expected_status='Pass',executed_field='executed',minimum_executed=136)
    metadata=run('cargo-metadata',['cargo','metadata','--format-version=1','--offline','--locked'])
    metadata_path=out/'cargo-metadata.json';metadata_path.write_text(metadata)
    run('architecture',['node','tools/check-architecture.mjs','--metadata',str(metadata_path)])
    run('plan',[sys.executable,'tools/check-plan.py','--self-test'])
    run('manifest-integrity',[sys.executable,'tools/check-conformance.py'])
    run('handoff',['node','tools/handoff.mjs','--check'],expected_status='Pass')
    run('tool-tests',['node','--test','tests/architecture.test.mjs','tests/hash-syntax.test.mjs','tests/handoff.test.mjs','tests/report.test.mjs'])
    protocol_reports={}
    for owner in ['B0','T01','T04','T05']:
        protocol_reports[owner]=run('protocol-'+owner,[sys.executable,'tools/run-cases.py','--task',owner,'--manifest','validation/task-test-plans.json','--out-root',str(out/('protocol-'+owner)/'campaign')],required=owner in ['B0','T01'])
    # Whole T04/T05 reports must remain Fail while their other fixtures are
    # missing. Nevertheless, regressions in implemented fixtures block this
    # draft. A fixed floor prevents deleting a fixture from removing the gate.
    fixed={'PLAN-T04-002','PLAN-T04-004','PLAN-T05-003','PLAN-T05-004'}
    plan=json.loads((ROOT/'validation/task-test-plans.json').read_text())
    required_cases={c['id']:c for c in plan['cases'] if c['owner'] in ['T04','T05'] and (c['id'] in fixed or (ROOT/c['required_fixture']).is_file())}
    observed={};errors=[]
    for owner in ['T04','T05']:
        try:
            path=(ROOT/json.loads(protocol_reports[owner])['result_path']).resolve()
            if not path.is_relative_to(ROOT):raise ValueError('result escaped project')
            report=json.loads(path.read_text())
            if report.get('task')!=owner:raise ValueError('wrong task report')
            for case in report['cases']:
                if case['id'] in observed:raise ValueError('duplicate case ID '+case['id'])
                observed[case['id']]=case
        except (KeyError,ValueError,OSError) as ex:errors.append(owner+': '+str(ex))
    for case_id in sorted(fixed-set(required_cases)):errors.append(case_id+': missing registration')
    details=[]
    for case_id,case in sorted(required_cases.items()):
        actual=observed.get(case_id,{})
        passed=(ROOT/case['required_fixture']).is_file() and actual.get('status')=='Pass' and actual.get('executed',0)>=case.get('minimum_executed',1) and actual.get('assertions',0)>=case.get('minimum_assertions',1)
        details.append({'id':case_id,'passed':passed,'executed':actual.get('executed',0),'assertions':actual.get('assertions',0)})
        if not passed:errors.append(case_id+': missing or nonpassing implemented fixture')
    derived={'name':'implemented-protocol','required_for_scalar_draft':True,'passed':not errors,'exit_code':int(bool(errors)),'kind':'Assertions over this run\'s complete protocol reports; no second execution','cases':details,'errors':errors}
    folder=out/'implemented-protocol';folder.mkdir();save(folder/'result.json',derived);records.append(derived)
    print(json.dumps({'check':derived['name'],'exit_code':derived['exit_code'],'required_for_scalar_draft':True}),flush=True)
    conformance_reports={}
    for owner in ['T01','T02','T04','T08']:
        conformance_reports[owner]=run('conformance-'+owner,[sys.executable,'tools/run-cases.py','--task',owner,'--manifest','conformance/index.json','--out-root',str(out/('conformance-'+owner)/'campaign')],required=owner in ['T01','T04'])
    from implemented_conformance import check
    derived=check(ROOT,conformance_reports)
    folder=out/'implemented-conformance';folder.mkdir();save(folder/'result.json',derived);records.append(derived)
    print(json.dumps({'check':derived['name'],'exit_code':derived['exit_code'],'required_for_scalar_draft':True}),flush=True)
    try:commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True,stderr=subprocess.DEVNULL).strip()
    except subprocess.CalledProcessError:commit=None
    required=[r for r in records if r['required_for_scalar_draft']]
    report={'schema_version':1,'run_id':stamp,'source_commit':commit,'scope':'Dynamic-stdin multi-module native application; public finite Copy aggregates/generics; CT CFG and aggregates; Fn/FnMut and thin callbacks; shared/exclusive references, reborrow and transfer; scalar C ABI with lexical unsafe; finite Managed/Desc/give; local owning/descriptor Fn/FnMut/FnOnce environments and normal and synchronous Copy error propagation cleanup with explicit lexical storage scopes; finite owning Package values with atomic preparation, whole movement, direct Managed field take/conditional cleanup/refill and Copy field store, partial local shape only, exact field descriptions and complete SSA/ABI boundaries; bounded nonescaping concrete Fn/FnMut/FnOnce helpers with public verified call contracts; finite self-contained impl Fn/FnMut/FnOnce factories with one concrete returned environment, finite Copy callback signatures, Copy or owned Managed/finite Package captures with persistent unique environment views, independent Shared write restrictions and full FnMut normal/error restoration, exact declared callable kind, and ordinary cross-module return, move, synchronous retained call and cleanup; lazy optional phase dumps; u8 and checked integer conversions in source/CT/LLVM; synchronous noescape borrowed C buffers with exact independent loan checks; moved Copy capture address mutation writeback on normal and error exits; dependency-driven resource-effect scheduling with full final validation; u16 and static opaque handles/UTF16/C callback values, explicit static reentry contracts, strict by-address C record layout, real Win64 LLVM/COFF/PE and pure ZIL Win32 window input/render/resize/cleanup under Wine compatibility execution, bounded Source CT preflight before cloning; finite Copy ordinary borrowed slices with checked ranges, returned views, closure-local windows and Linux/Win64 fat-pointer ABI; explicit sealed u8 ByteBuffer storage with reserve/push/borrow, failure-preserving allocation, unique cleanup, source generic slice composition and Linux/Win64 internal owned ABI; direct ByteBuffer owning Package fields, actual owned direct/Package captures, retained factories and flag-before-builtin cleanup; schema/wire 17; independent native scalar/resource-closure/byte-buffer differential hunt and bounded compiler/oracle checks','draft_gate':'Pass' if all(r['passed'] for r in required) else 'Fail','full_language_gate':'NotSatisfied','product_gate':'NotRun','required_checks':len(required),'required_passed':sum(r['passed'] for r in required),'all_recorded_checks':len(records),'rust_version':rust.strip(),'llvm_version':clang.strip(),'normative_hash':json.loads((ROOT/'data/normative-inputs.json').read_text())['normative_hash'],'records':records,'result_path':str((out/'result.json').relative_to(ROOT))}
    save(out/'result.json',report)
    if a.publish_summary:report=publish_summary(ROOT,report)
    print(json.dumps({k:v for k,v in report.items() if k not in ('records','rust_version','llvm_version')},ensure_ascii=False,indent=2))
    return report['draft_gate']!='Pass'
if __name__=='__main__':sys.exit(main())

from pathlib import Path
import importlib.util, json, contextlib, io

root=Path(__file__).parent
spec=importlib.util.spec_from_file_location('frozen_runtime_tests',root/'tools/research/compile-performance/test_runtime_bench.py')
module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
bench=module.bench
observations=[]

def compiler(argv,stdout,stderr,cpus,timeout):
    result=module.RuntimeControls.compiler(argv,stdout,stderr,cpus,timeout)
    report=json.loads(stdout.read_text())
    enabled='--closed-executable' in argv
    report['native_codegen']={'closed_executable':enabled,'internalized_functions':7 if enabled else 0}
    stdout.write_text(json.dumps(report))
    return result

def check(label, mutate):
    case=module.RuntimeControls();case.setUp();case.configured_arms()
    try:
        case.build_control(compiler)
        records=case.receipt.parent/'builds.jsonl'
        rows=[json.loads(line) for line in records.read_text().splitlines()]
        baseline=bench.built(case.plan,case.plan_path,case.receipt)
        assert set(baseline)=={('x','A'),('x','B')}
        mutate(case,rows)
        records.write_text(''.join(json.dumps(row)+'\n' for row in rows))
        receipt=json.loads(case.receipt.read_text())
        receipt['builds_sha256']=bench.sha(records)
        case.receipt.write_text(json.dumps(receipt))
        try:
            bench.built(case.plan,case.plan_path,case.receipt)
            observation={'case':label,'rejected':False}
        except SystemExit as error:
            observation={'case':label,'rejected':True,'reason':str(error)}
        observations.append(observation)
    finally:case.tearDown()

def argv_value(option,value):
    def mutate(case,rows):
        argv=rows[1]['argv'];argv[argv.index(option)+1]=value
    return mutate

for label,flag,value in [('optimization-drift','--opt','O0'),('jobs-drift','--codegen-jobs','1'),('units-drift','--codegen-units','1')]:
    check(label,argv_value(flag,value))
check('extra-overriding-optimization',lambda case,rows:rows[1]['argv'].extend(['--opt','O0']))

def raw_mode(value,remove=False):
    def mutate(case,rows):
        row=rows[1];path=case.receipt.parent/'logs'/row['stdout'];report=json.loads(path.read_text())
        if remove:del report['native_codegen']['closed_executable']
        else:report['native_codegen']['closed_executable']=value
        path.write_text(json.dumps(report));row['stdout_sha256']=bench.sha(path)
    return mutate
for label,value,remove in [('closed-ignored',False,False),('closed-missing',None,True),('closed-string','true',False),('closed-integer',1,False)]:
    check(label,raw_mode(value,remove))

case=module.RuntimeControls();case.setUp()
try:
    argv=bench.build_argv(case.plan,case.root,case.case,'A',case.executable)
    legacy=(argv[argv.index('--opt')+1]=='O0' and argv[argv.index('--codegen-units')+1]=='1'
        and argv[argv.index('--codegen-jobs')+1]=='1' and '--closed-executable' not in argv)
finally:case.tearDown()
result={'reviewed_commit':'ac5d7ca1d8cd3b1ef8949f18c354f9651ec81d86','reviewer':'/root/benchmark_audit',
        'scope':'Independent synthetic receipt mutations; no performance scores or compiler builds.',
        'author_controls_replayed':18,'independent_counterexamples':observations,'old_o0_default_preserved':legacy,
        'pass':legacy and all(o['rejected'] for o in observations),
        'findings':'Build argv binds per-arm optimization/units/jobs/flag; missing/ignored/non-boolean requested closed mode rejects after raw-log and receipt digests are recomputed. Existing tests separately retain single CPU, ABBA, weights and excluding compile/startup/warmup.'}
(root/'REVIEW.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))

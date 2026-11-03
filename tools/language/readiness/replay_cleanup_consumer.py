#!/usr/bin/env python3
"""Replay existing ZUI-language177 cleanup oracles through real LLVM API objects."""
import argparse, hashlib, importlib.util, json, os, pathlib, subprocess, sys, time

def digest(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--repo', type=pathlib.Path, required=True)
    p.add_argument('--cli', type=pathlib.Path, required=True)
    p.add_argument('--source-commit', required=True)
    p.add_argument('--cli-sha256', required=True)
    p.add_argument('--out', type=pathlib.Path, required=True)
    a = p.parse_args()
    repo = a.repo.resolve()
    cli = a.cli.resolve()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    assert digest(cli) == a.cli_sha256
    assert len(a.source_commit) == 40
    oracle = repo / 'tools/language/177/consumer.py'
    spec = importlib.util.spec_from_file_location('existing177', oracle)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    clang = pathlib.Path(os.environ.get('ZIL_CLANG', subprocess.check_output(['which', 'clang'], text=True).strip())).resolve()
    sources = repo / 'examples/mainline/zui_language177'
    report = {'status': 'Running', 'source_commit': a.source_commit, 'source_identity_kind': 'external observed build receipt; not binary self authentication', 'cli_sha256': digest(cli), 'clang_sha256': digest(clang), 'source_hashes': {p.name: digest(p) for p in sorted(sources.glob('*.zil'))}, 'oracle': 'tools/language/177/consumer.py scenarios (preexisting business oracle)', 'oracle_sha256': digest(oracle), 'builds': [], 'observations': [], 'commands': 0}

    def save():
        (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')

    def run(argv, name, data=b'', env=None):
        directory = out / name
        directory.mkdir(parents=True, exist_ok=True)
        start = time.time()
        result = subprocess.run([str(x) for x in argv], input=data, capture_output=True, env=dict(os.environ, **env or {}), timeout=120)
        (directory / 'stdout').write_bytes(result.stdout)
        (directory / 'stderr').write_bytes(result.stderr)
        (directory / 'record.json').write_text(json.dumps({'argv': [str(x) for x in argv], 'input_hex': data.hex(), 'environment': env or {}, 'returncode': result.returncode, 'seconds': time.time() - start}, indent=2) + '\n')
        report['commands'] += 1
        return result
    probe = out / 'allocator.o'
    compiled = run([clang, '-O0', '-c', repo / 'tools/language/173/allocator_probe.c', '-o', probe], 'prepare-probe')
    assert compiled.returncode == 0, compiled.stderr
    wrapper = out / 'link-probe.py'
    wrapper.write_text('#!' + sys.executable + '\nimport os,sys,json\ncmd=' + repr([str(clang)]) + '+sys.argv[1:]+' + repr([str(probe), '-Wl,--wrap=realloc', '-Wl,--wrap=free']) + '\nassert "ir" not in sys.argv and "-c" not in sys.argv, "probe must only link actual API objects"\nwith open(' + repr(str(out / 'probe-link-argv.jsonl')) + ',"a") as f:f.write(json.dumps(cmd)+"\\n")\nos.execv(cmd[0],cmd)\n')
    wrapper.chmod(493)
    report.update(probe_sha256=digest(repo / 'tools/language/173/allocator_probe.c'), link_wrapper_sha256=digest(wrapper))
    save()
    try:
        for units in [1, 4]:
            for name in ['main', 'field_exit']:
                for opt in ['O0', 'O2']:
                    for mode in ['ordinary', 'probe']:
                        label = f'u{units}/{name}/{opt}/{mode}'
                        exe = out / label / 'program'
                        exe.parent.mkdir(parents=True, exist_ok=True)
                        built = run([cli, 'build', sources / (name + '.zil'), '-o', exe, '--backend', 'llvm-api', '--opt', opt, '--codegen-units', str(units), '--codegen-jobs', '4', '--clang', wrapper if mode == 'probe' else clang, '--json'], label + '/build')
                        body = json.loads(built.stdout)
                        assert built.returncode == 0 and body['status'] == 'Success' and (body['verification_runs'] == 1), (label, body)
                        cg = body['native_codegen']
                        assert cg['backend'] == 'llvm-api' and cg['fallback'] is None, (label, cg)
                        assert cg['actual_units'] >= 1 and (units == 1 or cg['actual_units'] > 1), (label, cg)
                        for obj in cg['object_reports']:
                            assert obj['backend'] == 'llvm-api' and obj['input_kind'] == 'builder' and (obj['pipeline'] == 'default<' + opt + '>')
                        report['builds'].append({'name': label, 'llvm_sha256': digest(body['artifacts']['llvm']), 'executable_sha256': digest(exe), 'verification_runs': 1, 'codegen': cg})
                        for row in module.scenarios(name):
                            if mode == 'probe' and row.get('ordinary_only'):
                                continue
                            cases = [(0, row['stdout'], row['allocated'], row['exit'])]
                            if mode == 'probe':
                                cases += row['failures']
                            for fail, expected, allocated, exit_code in cases:
                                r = run([exe], label + '/' + row['name'] + f'-fail{fail}', row['input'], {'ZIL_TEST_FAIL_AT': str(fail)} if mode == 'probe' else {})
                                assert (r.returncode, r.stdout) == (exit_code, expected), (label, row['name'], fail, r.returncode, r.stdout, r.stderr)
                                allocation = None
                                if mode == 'probe':
                                    allocation = json.loads(r.stderr)
                                    assert allocation['live'] == 0 and allocation['allocated'] == allocated and (allocation['frees'] == allocated) and (allocation['failures'] == int(fail > 0)), (label, allocation)
                                else:
                                    assert not r.stderr, (label, r.stderr)
                                report['observations'].append({'case': label + '/' + row['name'], 'fail_at': fail, 'exit': r.returncode, 'stdout': r.stdout.decode(), 'allocation': allocation})
                        save()
        assert len(report['builds']) == 16 and len(report['observations']) == 128
        report['status'] = 'Pass'
    except Exception as error:
        report.update(status='Fail', error=repr(error))
        save()
        raise
    save()
    print(json.dumps({'status': report['status'], 'builds': len(report['builds']), 'executions': len(report['observations']), 'injected_failures': sum((x['fail_at'] > 0 for x in report['observations']))}))
if __name__ == '__main__':
    main()

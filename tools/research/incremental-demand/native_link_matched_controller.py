#!/usr/bin/env python3
"""Prepare or run exact frozen main9170/candidate/Rust check+O2 windows.

Run only in the root-assigned approved outer execution window: native linker
profiles require user/PID/network namespaces. Default prepares commands without
launching any compiler. Source/binary/runtime pins are supplied after freeze.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
RUNNER = Path(__file__).with_name('native_link_matched_measure.py')
HELPERS = Path(__file__).parent
MAIN = '9170bb4210554392e36eb7528596612596ebef80'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def validate_pins(pins):
    require(pins['schema'] == 'native-link-three-arm-pins-v1', 'pin schema')
    require(pins['baseline']['source'] == MAIN, 'baseline is not the requested exact main9170')
    require(type(pins['tool_source']) is str and re.fullmatch('[0-9a-f]{40}', pins['tool_source']) is not None, 'exact tools source required')
    for arm in ('baseline', 'candidate'):
        item = pins[arm]
        require(type(item['source']) is str and re.fullmatch('[0-9a-f]{40}', item['source']) is not None, arm+' exact source required')
        for name in ('binary_sha256', 'llvm_dso_sha256'):
            require(type(item[name]) is str and re.fullmatch('[0-9a-f]{64}', item[name]) is not None, arm+' exact '+name+' required')
        for name in ('binary', 'worktree', 'build_receipt', 'llvm_library_path'):
            require(type(item[name]) is str and Path(item[name]).is_absolute() and '..' not in Path(item[name]).parts, arm+' absolute '+name+' required')
    require(pins['native_link_profile'] == '/workspace/.zil-environment/native-link-v1/manifest'
            and type(pins['native_link_profile_sha256']) is str and re.fullmatch('[0-9a-f]{64}', pins['native_link_profile_sha256']) is not None,
            'exact installed native profile pin required')


def runner_command(pins, mode, out, blocks, run=False, publication_recovery=False):
    validate_pins(pins)
    require(mode in ('check', 'o2') and blocks in (2, 6), 'mode/block protocol')
    command = ['python3', '-B', str(RUNNER), '--repo', str(ROOT), '--out', str(out), '--mode', mode,
               '--blocks', str(blocks), '--tool-source', pins['tool_source'],
               '--native-link-profile', pins['native_link_profile'],
               '--native-link-profile-sha256', pins['native_link_profile_sha256'],
               '--candidate-extra-arg=--push-reuse']
    for arm in ('baseline', 'candidate'):
        item = pins[arm]
        for name in ('binary', 'source', 'worktree', 'build_receipt', 'llvm_library_path'):
            option = '--'+arm if name == 'binary' else '--'+arm+'-'+name.replace('_', '-')
            command += [option, item[name]]
        command += ['--expected-'+arm+'-binary-sha256', item['binary_sha256'],
                    '--expected-'+arm+'-dso-sha256', item['llvm_dso_sha256']]
    if run:
        command.append('--run')
    if publication_recovery and mode == 'o2':
        command.append('--publication-recovery')
    return command


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pins', required=True, type=Path)
    parser.add_argument('--out-prefix', required=True, type=Path)
    parser.add_argument('--blocks', type=int, choices=(2, 6), default=2)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--publication-recovery', action='store_true', help='explicit optional last-native-block correctness phase, excluded from the nine cells')
    args = parser.parse_args()
    pins = json.loads(args.pins.read_text())
    validate_pins(pins)
    prefix = args.out_prefix.resolve()
    prefix.parent.mkdir(parents=True, exist_ok=True)
    identity = {str(path): digest(path) for path in [RUNNER, Path(__file__), *[HELPERS/name for name in ('rust_compare.py', 'push_counters.py', 'matched_validate.py')]]}
    require(len(identity) == 5, 'runner/controller/three fixed helper files required')
    if args.run:
        require(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == pins['tool_source'], 'tool source has changed since freeze')
        require(not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip(), 'root must be frozen and clean')
        require(digest(pins['native_link_profile']) == pins['native_link_profile_sha256'], 'installed profile manifest changed')
        for arm in ('baseline', 'candidate'):
            require(digest(pins[arm]['binary']) == pins[arm]['binary_sha256'], arm+' binary differs from frozen SHA')
        require(not os.environ.get('LD_PRELOAD') and not os.environ.get('LD_AUDIT'), 'loader interposition is not supported')
        os.environ.pop('ZIL_LLVM_CONFIG', None)
        os.environ.pop('LLVM_CONFIG', None)
        os.environ.pop('LD_LIBRARY_PATH', None)
        os.environ.pop('ZIL_LLVM_LIB_DIR', None)
        os.environ['ZIL_CLANG'] = '/workspace/.zil-environment/llvm-23.1.4/bin/clang'
    for mode in ('check', 'o2'):
        out = prefix.with_name(prefix.name+'-'+mode)
        record = out.with_name(out.name+'-command.json')
        log, exit_path = out.with_suffix('.log'), out.with_suffix('.exit')
        require(not any(path.exists() for path in (out, record, log, exit_path)), 'unique run paths required')
        command = runner_command(pins, mode, out, args.blocks, args.run, args.publication_recovery)
        with record.open('x') as stream:
            json.dump({'schema': 'native-link-three-arm-controller-v1', 'command': command, 'pins': pins,
                       'controller_and_runner_sha256': identity, 'blocks': args.blocks,
                       'launcher_environment_policy': 'Rust has no inherited Zil LLVM loader path; LLVM_CONFIG/ZIL_LLVM_CONFIG unset. Each Zil resident receives its separately pinned LLVM directory. ZIL_CLANG is the installed LLVM23.1.4 driver.',
                       'scope': 'One sequential main9170/current candidate/Rust 1.99.1 window per mode; full nine original425 scenarios and parents; no prior-candidate comparison. Successful outputs retained in all arms; any output mutations are separate correctness only.'}, stream, indent=2)
            stream.write('\n')
        if not args.run:
            print(json.dumps({'prepared_only': True, 'command_record': str(record), 'compiler_processes_started': 0}))
            continue
        shutil.copyfile(__file__, out.with_name(out.name+'-controller.py'))
        with log.open('x') as stream:
            result = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
        with exit_path.open('x') as stream:
            stream.write(str(result.returncode)+'\n')
        print(json.dumps({'mode': mode, 'exit': result.returncode, 'out': str(out)}), flush=True)
        if result.returncode:
            return result.returncode
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

#!/usr/bin/env python3
"""Record a bounded, post-build tool/config observation without rewriting BUILD."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def identity(path):
    path = Path(path).resolve()
    data = path.read_bytes()
    return dict(path=str(path), sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))


def command(argv, cwd, environment):
    result = subprocess.run(argv, cwd=cwd, env=environment, stdin=subprocess.DEVNULL,
                            capture_output=True, text=True, timeout=20)
    return dict(command=argv, exit=result.returncode,
                stdout=result.stdout, stderr=result.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', type=Path, required=True)
    parser.add_argument('--environment-script', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not args.out.exists(), 'supplements are immutable; select a fresh path'
    build_bytes = args.build.read_bytes()
    build = json.loads(build_bytes)
    assert build['status'] == 'Pass'
    repo = Path(build['build']['cwd']).resolve()
    environment = os.environ.copy()
    environment.update(build['build']['environment_overrides'])
    selected = ['CARGO_INCREMENTAL', 'RUSTUP_TOOLCHAIN', 'RUSTC', 'RUSTC_WRAPPER',
                'RUSTC_WORKSPACE_WRAPPER', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
                'CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS']
    fields = ['OPT_LEVEL', 'DEBUG', 'SPLIT_DEBUGINFO', 'STRIP', 'DEBUG_ASSERTIONS',
              'OVERFLOW_CHECKS', 'LTO', 'PANIC', 'INCREMENTAL', 'CODEGEN_UNITS', 'RPATH']
    for profile in ['RELEASE', 'DEV', 'TEST', 'BENCH']:
        selected += ['CARGO_PROFILE_' + profile + '_' + field for field in fields]
        selected += ['CARGO_PROFILE_' + profile + '_BUILD_OVERRIDE_' + field for field in fields]
    tools = {}
    for name in ['cargo', 'rustc']:
        executable = shutil.which(name, path=environment.get('PATH'))
        assert executable, name
        dispatch = command(['rustup', 'which', name], repo, environment)
        assert dispatch['exit'] == 0, dispatch
        resolved = Path(dispatch['stdout'].strip()).resolve()
        version = command([str(resolved), '-vV'], repo, environment)
        assert version['exit'] == 0, version
        tools[name] = dict(path_entry=str(Path(executable).absolute()),
                           path_entry_resolved=identity(executable),
                           rustup_resolution=dispatch, tool=identity(resolved), version=version)
    directories = [repo, *repo.parents]
    config_paths = [directory / '.cargo' / filename for directory in directories
                    for filename in ['config', 'config.toml']]
    cargo_home = Path(environment.get('CARGO_HOME', str(Path.home() / '.cargo')))
    config_paths += [cargo_home / filename for filename in ['config', 'config.toml']]
    configs = []
    for path in dict.fromkeys(config_paths):
        configs.append(dict(requested_path=str(path), exists=path.is_file(),
                            identity=identity(path) if path.is_file() else None))
    assert args.build.read_bytes() == build_bytes
    receipt = dict(schema=1, status='Observed', observed_at_utc=datetime.now(timezone.utc).isoformat(),
                   producer=identity(__file__), build=identity(args.build),
                   environment_script=identity(args.environment_script), tools=tools,
                   selected_environment={name: environment.get(name) for name in selected},
                   cargo_config_candidates=configs,
                   boundary='Observed after the completed build using the same environment script and recorded explicit overrides. Tool dispatch, selected inherited variables and config existence were not captured at the original cargo Popen; this supplement does not claim retroactive build-time observation. BUILD, its before/after source comparison, producer bytes and compiled ELF remain unchanged. Config contents and the full environment are not recorded.')
    args.out.write_text(json.dumps(receipt, indent=2) + '\n')


if __name__ == '__main__':
    main()

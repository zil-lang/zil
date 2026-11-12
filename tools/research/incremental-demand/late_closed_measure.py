#!/usr/bin/env python3
"""Original paired workload with an explicit post-O2 closed-output quality gate.

Default native_measure remains ELF-byte-equal. This separate protocol preserves
that observation and requires retained machine code equality plus the exact
current deletion receipt. It is not the historical byte-equality protocol.
"""
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / 'reactive-query-graph'))
import native_measure
import matched_measure
from elf_instruction_quality import compare_pair


class ClosedOutputQuality:
    def validate_arguments(self, args):
        if (args.mode != 'O2' or args.backend != 'llvm-api' or args.case != 'scalar'
                or (args.modules, args.functions, args.steps) != (20, 20, 16)
                or args.codegen_units != 16 or args.codegen_jobs != 1
                or args.request is not None or args.cache != 'all' or args.capacity != 256
                or args.source_cache or args.extra_arg or args.baseline_extra_arg
                or not set(args.candidate_extra_arg).issubset(
                    {'--late-closed-executable', '--push-reuse', '--profile-compilation'})
                or args.closed_executable
                or '--late-closed-executable' not in args.candidate_extra_arg
                or '--late-closed-executable' in args.extra_arg + args.baseline_extra_arg
                or '--closed-executable' in args.extra_arg + args.baseline_extra_arg + args.candidate_extra_arg):
            raise ValueError('late closed protocol requires original425/O2/CGU16/jobs1 and candidate-only late contract')
        nm = shutil.which('nm')
        if nm is None:
            raise ValueError('actual object inventory requires nm')
        args.nm = str(Path(nm).resolve())

    def describe(self, args):
        files = []
        for name in ['late_closed_measure.py', 'elf_instruction_quality.py', 'matched_measure.py', 'rust_compare.py']:
            source = HERE / name
            data = source.read_bytes()
            target = args.out / name
            target.write_bytes(data)
            files.append({'path': name, 'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)})
        nm = Path(args.nm)
        version = subprocess.run([str(nm), '--version'], capture_output=True, check=True)
        return {'schema': 'post-o2-closed-machine-quality-v1',
                'nm': {'path': str(nm), 'sha256': native_measure.digest(nm),
                       'version_stdout_hex': version.stdout.hex(), 'version_stderr_hex': version.stderr.hex()},
                'contract': 'Candidate explicitly grants no generated ZIL symbol exports. Original O2 and CGU policy are retained; source-private classification is not inferred.',
                'files': files,
                'checks': 'Current Verify, exact raw input diagnostics except the preexisting push identity comment, full native oracle, actual retained machine instructions and authorized current removals. ELF byte equality remains separately reported; unknown normalization fails.'}

    def compare_pair(self, pair, args):
        for row in pair:
            report = row['resident']['report']
            program = Path(report['artifacts']['executable'])
            row['inventory'] = matched_measure.zil_inventory(args, report, program.parent / 'objects')
        return compare_pair({row['arm']: row for row in pair}, args.out)


if __name__ == '__main__':
    native_measure.main(native_quality=ClosedOutputQuality())

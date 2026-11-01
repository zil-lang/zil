#!/usr/bin/env python3
"""Small real-IR controls for the two isolated JumpThreading plugin arms."""
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path


def chain(length):
    lines = ['declare void @llvm.trap()', 'define i32 @chain(i32 %x) {',
             'entry:', '  br label %b0']
    for i in range(length):
        operand = '%x' if i == 0 else f'%v{i-1}'
        target = f'b{i+1}' if i + 1 < length else 'done'
        lines += [f'b{i}:', f'  %v{i} = add i32 {operand}, 1',
                  f'  br i1 true, label %{target}, label %trap']
    lines += ['done:', f'  ret i32 %v{length-1}', 'trap:',
              '  call void @llvm.trap()', '  unreachable', '}']
    return '\n'.join(lines) + '\n'


CASES = {
    'chain_32': chain(32),
    'chain_96': chain(96),
    'single_phi': '''define i32 @single_phi(i32 %x) {
entry:
  br label %body
body:
  %p = phi i32 [ %x, %entry ]
  %q = add i32 %p, 3
  ret i32 %q
}
''',
    'loop': '''define i32 @loop(i32 %n) {
entry:
  br label %pre
pre:
  br label %head
head:
  %i = phi i32 [ 0, %pre ], [ %next, %latch ]
  %go = icmp slt i32 %i, %n
  br i1 %go, label %latch, label %done
latch:
  %next = add i32 %i, 1
  br label %head
done:
  ret i32 %i
}
''',
    'multi_pred_phi': '''define i32 @multi_pred_phi(i1 %c) {
entry:
  br i1 %c, label %left, label %right
left:
  br label %merge
right:
  br label %merge
merge:
  %v = phi i32 [ 7, %left ], [ 9, %right ]
  %test = icmp eq i32 %v, 7
  br i1 %test, label %yes, label %no
yes:
  ret i32 1
no:
  ret i32 2
}
''',
    'address_taken': '''@saved = global ptr blockaddress(@address_taken, %body)
define i32 @address_taken(i32 %x) {
entry:
  br label %body
body:
  ret i32 %x
}
''',
    'unreachable_component': '''define i32 @unreachable_component(i32 %x) {
entry:
  ret i32 %x
dead_a:
  br label %dead_b
dead_b:
  br label %dead_a
}
''',
    'nonreturn_assume': '''declare void @may_not_return()
declare void @llvm.assume(i1)
define i32 @nonreturn_assume(i1 %known, i32 %x) {
entry:
  call void @may_not_return()
  call void @llvm.assume(i1 %known)
  br label %body
body:
  br i1 %known, label %yes, label %no
yes:
  ret i32 %x
no:
  ret i32 7
}
''',
    'duplicate_switch_edges': '''define i32 @duplicate_switch_edges(i32 %x) {
entry:
  switch i32 %x, label %merge [ i32 0, label %merge
                              i32 1, label %other ]
merge:
  ret i32 %x
other:
  ret i32 0
}
''',
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--opt', type=Path, required=True)
    parser.add_argument('--plugin', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    receipt = {
        'scope': 'small LLVM IR controls; no large benchmark and no timing claims',
        'opt_sha256': sha(args.opt), 'plugin_sha256': sha(args.plugin),
        'control_script_sha256': sha(Path(__file__)), 'rows': [],
        'uncovered': ['native execution (author harness owns this)',
                      'convergence-token and EH transformations',
                      'MSan/profile-metadata combinations'],
    }
    for name, source in CASES.items():
        source_path = args.output / (name + '.ll')
        source_path.write_text(source)
        outputs = {}
        row = {'case': name, 'source_sha256': sha(source_path), 'arms': {}}
        for arm, pass_name in [('builtin', 'jump-threading'),
                               ('original', 'zil-jt-original'),
                               ('entry', 'zil-jt-entry')]:
            output = args.output / (name + '.' + arm + '.ll')
            command = [str(args.opt), '-load-pass-plugin', str(args.plugin),
                       '-verify-each', '-passes=function(' + pass_name +
                       ',verify<domtree>)', '-S', str(source_path), '-o', str(output)]
            result = subprocess.run(command, capture_output=True)
            (args.output / (name + '.' + arm + '.stderr')).write_bytes(result.stderr)
            row['arms'][arm] = {'command': command, 'returncode': result.returncode}
            if result.returncode == 0:
                outputs[arm] = output.read_text()
                row['arms'][arm]['output_sha256'] = sha(output)
        row['all_verify_passed'] = len(outputs) == 3
        row['original_matches_builtin'] = (
            'original' in outputs and outputs.get('builtin') == outputs['original'])
        if name in ('chain_32', 'chain_96', 'single_phi', 'loop', 'nonreturn_assume'):
            row['entry_label_preserved'] = bool(
                re.search(r'^entry:', outputs.get('entry', ''), re.MULTILINE))
        if name == 'address_taken':
            row['address_reference_preserved'] = (
                'blockaddress(@address_taken, %body)' in outputs.get('entry', ''))
        row['valid'] = (row['all_verify_passed'] and row['original_matches_builtin']
                        and row.get('entry_label_preserved', True)
                        and row.get('address_reference_preserved', True))
        receipt['rows'].append(row)
    receipt['valid'] = all(row['valid'] for row in receipt['rows'])
    (args.output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({'valid': receipt['valid'], 'cases': len(receipt['rows']),
                      'failed': [row['case'] for row in receipt['rows'] if not row['valid']],
                      'receipt': str(args.output / 'result.json')}))
    raise SystemExit(0 if receipt['valid'] else 1)


if __name__ == '__main__':
    main()

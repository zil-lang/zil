#!/usr/bin/env python3
"""Frozen representative inputs; independent expected outputs, no compiler oracle."""
from pathlib import Path
import importlib.util
import shutil
ROOT = Path(__file__).resolve().parents[2]
GROUP = {'small': .10, 'medium': .20, 'large': .35, 'real_language': .35}


def load_existing():
    spec = importlib.util.spec_from_file_location('existing_compiler_corpus', ROOT/'tools/research/compiler-differential/corpus.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def changed(text, before, after):
    if text.count(before) != 1:
        raise ValueError(('edit anchor count', before, text.count(before)))
    return text.replace(before, after)


def single_source(snapshot):
    parts = [f'fn leaf{i}()->i64 {{ return {i % 17}; }}\n' for i in range(8192)]
    for i in range(256):
        expression = ' + '.join(f'leaf{j}()' for j in range(i*32, (i+1)*32))
        parts.append(f'fn part{i}()->i64 {{ return {expression}; }}\n')
    for i in range(8):
        expression = ' + '.join(f'part{j}()' for j in range(i*32, (i+1)*32))
        parts.append(f'fn group{i}()->i64 {{ return {expression}; }}\n')
    parts.append('fn total()->i64 { return ' + ' + '.join(f'group{i}()' for i in range(8)) + '; }\n')
    bias = 7 if snapshot == 'base' else 11
    parameter = ', extra:i64' if snapshot == 'public' else ''
    extra = '+extra' if snapshot == 'public' else ''
    parts.append(f'fn adjust(x:i64{parameter})->i64 {{ var y=x; var i:i64=0; while i<3 {{ y=(y+i+{bias}{extra})%251; i+=1; }} return y; }}\n')
    parts.append('extern "C" fn getchar()->i32;\nextern "C" fn putchar(x:i32)->i32;\n'
                 'fn read_byte()->i32 { unsafe { return getchar() } }\n'
                 'fn write_byte(x:u8) { unsafe { putchar(i32(x)) } }\n')
    argument = ',3' if snapshot == 'public' else ''
    parts.append('fn main()->i32 { var x=read_byte(); while x>=0 { let y=(adjust(i64(x)' + argument + ')+total())%251; write_byte(u8(y)); x=read_byte(); } return 0; }\n')
    return ''.join(parts)


def generate(out):
    existing = load_existing()
    cases = existing.generate(out)
    for case in cases:
        case['group'] = 'real_language' if case['id'] == 'retained' else case['group']
        case['entry'] = None
    variants = {}
    base = 'fn helper(x:i64)->i64 { return x+7; }\npub fn answer()->i64 { return helper(0); }\n'
    private = changed(base, 'x+7', 'x+11')
    public = changed(changed(changed(private, 'helper(x:i64)', 'helper(x:i64,extra:i64)'), 'x+11', 'x+11+extra'), 'helper(0)', 'helper(0,3)')
    variants['minimal'] = ('small', 'answer', (base, private, public), (7,11,14), 'generated minimal used helper')
    base = (ROOT/'examples/generics/main.zil').read_text()
    private = changed(base, 'let left: i64 = 4000', 'let left: i64 = 4100')
    public = changed(private, 'relay<T>(value: T, count: i32)', 'relay<T>(value: T, count: i32, step: i32)')
    public = changed(public, 'relay::<T>(value, count - 1)', 'relay::<T>(value, count - step, step)')
    public = changed(public, 'relay(identity(chosen), 4)', 'relay(identity(chosen), 4, 1)')
    variants['generics'] = ('real_language', 'answer', (base, private, public), (4023,4123,4123), 'examples/generics/main.zil')
    base = (ROOT/'examples/comptime/main.zil').read_text()
    private = changed(base, '#(fib(12))', '#(fib(13))')
    public = changed(private, 'sum_even(n: i64)', 'sum_even(n: i64, extra: i64)')
    public = changed(public, 'return sum\n', 'return sum + extra\n')
    public = changed(public, '#(sum_even(11))', '#(sum_even(11, 3))')
    variants['comptime'] = ('real_language', 'answer', (base, private, public), (180,269,272), 'examples/comptime/main.zil')
    base = (ROOT/'examples/aggregates/main.zil').read_text()
    private = changed(base, 'adjust(initial, 2)', 'adjust(initial, 3)')
    public = changed(private, 'total(order: Order)', 'total(order: Order, extra: i64)')
    public = changed(public, '- order.discount\n', '- order.discount + extra\n')
    public = changed(public, 'total(initial)', 'total(initial, 1)')
    public = changed(public, 'total(changed)', 'total(changed, 1)')
    variants['aggregates'] = ('real_language', 'result', (base, private, public), (6483,6495,6596), 'examples/aggregates/main.zil')
    for name, (group, entry, texts, answers, origin) in variants.items():
        root = out/'corpus'/name
        for snapshot, text in zip(('base','private','public'), texts):
            path = root/snapshot/'zil'/'main.zil'; path.parent.mkdir(parents=True); path.write_text(text)
        io = root/'io'; io.mkdir(); (io/'input.bin').write_bytes(b'')
        expected = {}
        for snapshot, answer in zip(('base','private','public'), answers):
            path = io/f'{snapshot}.bin'; path.write_bytes(f'{answer}\n'.encode()); expected[snapshot] = str(path.relative_to(out))
        cases.append({'id':name, 'group':group, 'entry':entry, 'root':str(root.relative_to(out)), 'kind':origin,
                      'inputs':[{'path':str((io/'input.bin').relative_to(out)), 'expected':expected, 'bytes':0}]})
    root = out/'corpus'/'single8192'
    for snapshot in ('base','private','public'):
        path=root/snapshot/'zil'/'main.zil'; path.parent.mkdir(parents=True); path.write_text(single_source(snapshot))
    inputs=[]
    for size in (0,7,256):
        path=root/'io'/f'input-{size}.bin'; path.parent.mkdir(exist_ok=True)
        data=bytes((i*73+19)%256 for i in range(size)); path.write_bytes(data); expected={}
        for snapshot in ('base','private','public'):
            bias=7 if snapshot=='base' else 11; extra=3 if snapshot=='public' else 0
            output=bytes((byte+3*bias+3+3*extra+sum(i%17 for i in range(8192)))%251 for byte in data)
            dst=path.with_name(f'expected-{snapshot}-{size}.bin'); dst.write_bytes(output); expected[snapshot]=str(dst.relative_to(out))
        inputs.append({'path':str(path.relative_to(out)), 'expected':expected, 'bytes':size})
    cases.append({'id':'single8192','group':'large','entry':None,'root':str(root.relative_to(out)),
                  'kind':'generated single file; 8192 constant leaf functions, hierarchical callers, runtime input','inputs':inputs})
    counts={group:sum(c['group']==group for c in cases) for group in GROUP}
    for case in cases:
        case['weight']=GROUP[case['group']]/counts[case['group']]
        files=list((out/case['root']/'base'/'zil').rglob('*.zil'))
        case['zil_source']={'files':len(files),'bytes':sum(p.stat().st_size for p in files),'fn_tokens':sum(p.read_text().count('fn ') for p in files)}
    assert len(cases)==10 and abs(sum(c['weight'] for c in cases)-1)<1e-9
    return cases

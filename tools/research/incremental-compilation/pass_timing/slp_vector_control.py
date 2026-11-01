#!/usr/bin/env python3
"""Prove effective SLP selection through the pinned LLVM C API, in a fresh process."""
import argparse
import ctypes as C
import hashlib
import json
import re
import subprocess
from pathlib import Path

from build_probe import identity


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--llvm', type=Path, required=True)
    parser.add_argument('--clang', type=Path, required=True)
    parser.add_argument('--slp', choices=['on', 'off'], required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    llvm = C.CDLL(str(args.llvm.resolve()))
    ptr, integer, string = C.c_void_p, C.c_int, C.c_char_p

    def api(name, result, *parameters):
        fn = getattr(llvm, name)
        fn.restype = result
        fn.argtypes = parameters
        return fn

    version = [C.c_uint() for _ in range(3)]
    api('LLVMGetVersion', None, *([C.POINTER(C.c_uint)] * 3))(*map(C.byref, version))
    assert tuple(v.value for v in version) == (23, 1, 4)
    options = (string * 3)(b'zil-slp-vector-control', b'-time-passes',
                           b'-vectorize-slp=' + (b'true' if args.slp == 'on' else b'false'))
    api('LLVMParseCommandLineOptions', None, integer, C.POINTER(string), string)(3, options, b'Isolated SLP control')
    for suffix in ['TargetInfo', 'Target', 'TargetMC', 'AsmPrinter', 'AsmParser']:
        api('LLVMInitializeX86' + suffix, None)()
    context = api('LLVMContextCreate', ptr)()
    lines = ['target triple = "x86_64-unknown-linux-gnu"',
             'define void @slp_control(ptr noalias %a, ptr noalias %b, ptr noalias %out) {']
    for i in range(8):
        lines += [f'%ap{i} = getelementptr float, ptr %a, i64 {i}',
                  f'%bp{i} = getelementptr float, ptr %b, i64 {i}',
                  f'%op{i} = getelementptr float, ptr %out, i64 {i}',
                  f'%av{i} = load float, ptr %ap{i}, align 4',
                  f'%bv{i} = load float, ptr %bp{i}, align 4',
                  f'%sum{i} = fadd float %av{i}, %bv{i}',
                  f'store float %sum{i}, ptr %op{i}, align 4']
    lines += ['ret void', '}', 'define float @scalar_control(float %a, float %b) {',
              '%sum = fadd float %a, %b', 'ret float %sum', '}']
    ir = ('\n'.join(lines) + '\n').encode()
    (args.out / 'input.ll').write_bytes(ir)
    buffer = api('LLVMCreateMemoryBufferWithMemoryRangeCopy', ptr, string, C.c_size_t, string)(ir, len(ir), b'slp-control')
    module, error, target = ptr(), string(), ptr()
    assert api('LLVMParseIRInContext', integer, ptr, ptr, C.POINTER(ptr), C.POINTER(string))(context, buffer, C.byref(module), C.byref(error)) == 0, error.value
    assert api('LLVMGetTargetFromTriple', integer, string, C.POINTER(ptr), C.POINTER(string))(b'x86_64-unknown-linux-gnu', C.byref(target), C.byref(error)) == 0, error.value
    # LLVMCodeGenLevelDefault=2, LLVMRelocPIC=2, LLVMCodeModelDefault=0.
    machine = api('LLVMCreateTargetMachine', ptr, ptr, string, string, string, integer, integer, integer)(target, b'x86_64-unknown-linux-gnu', b'x86-64', b'', 2, 2, 0)
    assert machine
    layout = api('LLVMCreateTargetDataLayout', ptr, ptr)(machine)
    api('LLVMSetModuleDataLayout', None, ptr, ptr)(module, layout)
    verify = api('LLVMVerifyModule', integer, ptr, integer, C.POINTER(string))
    assert verify(module, 2, C.byref(error)) == 0, error.value
    pass_options = api('LLVMCreatePassBuilderOptions', ptr)()
    for name in ['LoopInterleaving', 'LoopVectorization', 'SLPVectorization']:
        api('LLVMPassBuilderOptionsSet' + name, None, ptr, integer)(pass_options, 1)
    api('LLVMPassBuilderOptionsSetMergeFunctions', None, ptr, integer)(pass_options, 1)
    failure = api('LLVMRunPasses', ptr, ptr, string, ptr, ptr)(module, b'default<O2>', machine, pass_options)
    assert not failure, 'LLVMRunPasses error'
    assert verify(module, 2, C.byref(error)) == 0, error.value
    raw_text = api('LLVMPrintModuleToString', ptr, ptr)(module)
    optimized = C.string_at(raw_text).decode()
    api('LLVMDisposeMessage', None, ptr)(raw_text)
    (args.out / 'optimized.ll').write_text(optimized)
    vector_body = re.search(r'define[^\n]*@slp_control\b.*?\n}', optimized, re.S).group()
    scalar_body = re.search(r'define[^\n]*@scalar_control\b.*?\n}', optimized, re.S).group()
    has_vector = bool(re.search(r'fadd <[248] x float>', vector_body))
    assert has_vector == (args.slp == 'on'), vector_body
    assert 'fadd float' in scalar_body and not re.search(r'<\d+ x ', scalar_body)
    object_buffer = ptr()
    assert api('LLVMTargetMachineEmitToMemoryBuffer', integer, ptr, ptr, integer, C.POINTER(string), C.POINTER(ptr))(machine, module, 1, C.byref(error), C.byref(object_buffer)) == 0, error.value
    data = api('LLVMGetBufferStart', ptr, ptr)(object_buffer)
    size = api('LLVMGetBufferSize', C.c_size_t, ptr)(object_buffer)
    (args.out / 'control.o').write_bytes(C.string_at(data, size))
    for name, handle in [('MemoryBuffer', object_buffer), ('PassBuilderOptions', pass_options),
                         ('Module', module), ('TargetData', layout), ('TargetMachine', machine)]:
        api('LLVMDispose' + name, None, ptr)(handle)
    api('LLVMContextDispose', None, ptr)(context)
    api('LLVMShutdown', None)()
    harness = '''#include <stdio.h>
extern void slp_control(const float *,const float *,float *);
extern float scalar_control(float,float);
int main(void){float a[8],b[8],o[8];for(int seed=0;seed<100;++seed){for(int i=0;i<8;++i){a[i]=(float)(seed*3+i);b[i]=(float)(seed-i);o[i]=-1;}slp_control(a,b,o);for(int i=0;i<8;++i)if(o[i]!=a[i]+b[i]||scalar_control(a[i],b[i])!=a[i]+b[i])return 7;}puts("ok");return 0;}
'''
    (args.out / 'harness.c').write_text(harness)
    command = [str(args.clang.resolve()), '-O0', str(args.out/'harness.c'), str(args.out/'control.o'), '-o', str(args.out/'control')]
    link = subprocess.run(command, capture_output=True)
    (args.out/'link.stderr').write_bytes(link.stderr)
    assert link.returncode == 0
    native = subprocess.run([str(args.out/'control')], capture_output=True)
    assert native.returncode == 0 and native.stdout == b'ok\n' and native.stderr == b''
    receipt = {'valid': True, 'scope': __doc__, 'global_vectorize_slp': args.slp,
               'vector_positive_contains_vector_fadd': has_vector, 'scalar_negative_stays_scalar': True,
               'verification_before_after': True, 'pipeline': 'default<O2>', 'merge_functions': 'after',
               'target': 'x86_64-unknown-linux-gnu/x86-64/PIC', 'llvm': identity(args.llvm),
               'clang': identity(args.clang), 'tool': identity(Path(__file__)),
               'input_sha256': hashlib.sha256(ir).hexdigest(), 'optimized_sha256': hashlib.sha256(optimized.encode()).hexdigest(),
               'object': identity(args.out/'control.o'), 'link_command': command, 'native_cases': 800,
               'executable': identity(args.out/'control'), 'native_stdout': native.stdout.decode()}
    (args.out/'result.json').write_text(json.dumps(receipt, indent=2)+'\n')
    print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()

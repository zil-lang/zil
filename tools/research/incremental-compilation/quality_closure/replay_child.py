#!/usr/bin/env python3
"""Research replay of verified captured IR with the original native API settings."""
import ctypes as C
import hashlib
import json
from pathlib import Path
import sys
import time


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    spec_path, output = map(Path, sys.argv[1:])
    output.mkdir(parents=True, exist_ok=False)
    spec = json.loads(spec_path.read_text())
    library = Path(spec['library']['path']).resolve()
    assert sha(library) == spec['library']['sha256']
    lib = C.CDLL(str(library), mode=C.RTLD_LOCAL)
    V, I, S, Z = C.c_void_p, C.c_int, C.c_char_p, C.c_size_t
    P = C.POINTER(V)
    def bind(name, result, args):
        function = getattr(lib, name)
        function.restype, function.argtypes = result, args
        return function
    ctx_new = bind('LLVMContextCreate', V, [])
    ctx_drop = bind('LLVMContextDispose', None, [V])
    buffer_new = bind('LLVMCreateMemoryBufferWithMemoryRangeCopy', V, [S, Z, S])
    parse = bind('LLVMParseIRInContext', I, [V, V, P, P])
    verify = bind('LLVMVerifyModule', I, [V, I, P])
    dispose_msg = bind('LLVMDisposeMessage', None, [V])
    mod_drop = bind('LLVMDisposeModule', None, [V])
    printmod = bind('LLVMPrintModuleToString', V, [V])
    gettarget = bind('LLVMGetTargetFromTriple', I, [S, P, P])
    newtm = bind('LLVMCreateTargetMachine', V, [V, S, S, S, I, I, I])
    droptm = bind('LLVMDisposeTargetMachine', None, [V])
    newlayout = bind('LLVMCreateTargetDataLayout', V, [V])
    droplayout = bind('LLVMDisposeTargetData', None, [V])
    setlayout = bind('LLVMSetModuleDataLayout', None, [V, V])
    settarget = bind('LLVMSetTarget', None, [V, S])
    newopts = bind('LLVMCreatePassBuilderOptions', V, [])
    dropopts = bind('LLVMDisposePassBuilderOptions', None, [V])
    runpasses = bind('LLVMRunPasses', V, [V, S, V, V])
    geterror = bind('LLVMGetErrorMessage', V, [V])
    droperror = bind('LLVMDisposeErrorMessage', None, [V])
    emit = bind('LLVMTargetMachineEmitToMemoryBuffer', I, [V, V, I, P, P])
    bufstart = bind('LLVMGetBufferStart', V, [V])
    bufsize = bind('LLVMGetBufferSize', Z, [V])
    bufdrop = bind('LLVMDisposeMemoryBuffer', None, [V])
    for name in ['LLVMInitializeX86TargetInfo', 'LLVMInitializeX86Target', 'LLVMInitializeX86TargetMC',
                 'LLVMInitializeX86AsmPrinter', 'LLVMInitializeX86AsmParser']:
        bind(name, None, [])()
    class DlInfo(C.Structure):
        _fields_ = [('filename', S), ('base', V), ('symbol', S), ('address', V)]
    libc, info = C.CDLL(None), DlInfo()
    libc.dladdr.argtypes, libc.dladdr.restype = [V, C.POINTER(DlInfo)], I
    assert libc.dladdr(C.cast(lib.LLVMRunPasses, V), C.byref(info))
    assert Path(info.filename.decode()).resolve() == library
    maps = [line for line in Path('/proc/self/maps').read_text().splitlines() if 'libLLVM' in line]
    assert {Path(line.split()[-1]).resolve() for line in maps} == {library}
    (output / 'loaded-maps.txt').write_text('\n'.join(maps) + '\n')
    settings = spec['settings']
    assert settings == dict(triple='x86_64-unknown-linux-gnu', cpu='x86-64', features='',
        codegen_level=2, relocation=2, code_model=0, pipeline='default<O2>',
        verify_each=0, loop_interleaving=1, loop_vectorization=1, slp_vectorization=1, merge_functions=1)
    target, message = V(), V()
    assert gettarget(settings['triple'].encode(), C.byref(target), C.byref(message)) == 0
    machine = newtm(target, settings['triple'].encode(), settings['cpu'].encode(),
                    settings['features'].encode(), 2, 2, 0)
    assert machine
    layout, options = newlayout(machine), newopts()
    assert layout and options
    for suffix, field in [('VerifyEach', 'verify_each'), ('LoopInterleaving', 'loop_interleaving'),
            ('LoopVectorization', 'loop_vectorization'), ('SLPVectorization', 'slp_vectorization'),
            ('MergeFunctions', 'merge_functions')]:
        bind('LLVMPassBuilderOptionsSet' + suffix, None, [V, I])(options, settings[field])
    receipt = dict(status='Running', scope=__doc__, producer_sha256=sha(__file__),
                   specification_sha256=sha(spec_path), library=spec['library'],
                   settings=settings, rows=[])
    def save():
        (output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    try:
        for item in spec['inputs']:
            source = Path(item['path'])
            assert sha(source) == item['sha256']
            context, module, error = ctx_new(), V(), V()
            row = dict(name=item['name'], input=item, valid=False)
            receipt['rows'].append(row)
            started = time.monotonic_ns()
            try:
                data = source.read_bytes()
                buffer = buffer_new(data, len(data), b'zil-unit')
                status = parse(context, buffer, C.byref(module), C.byref(error))
                if status:
                    message = C.string_at(error.value).decode() if error.value else 'parse failed'
                    dispose_msg(error)
                    raise RuntimeError(message)
                settarget(module, settings['triple'].encode())
                setlayout(module, layout)
                for phase in ['before', 'after']:
                    error = V()
                    status = verify(module, 2, C.byref(error))
                    message = C.string_at(error.value).decode() if error.value else ''
                    dispose_msg(error)
                    if status:
                        raise RuntimeError(phase + ' verification: ' + message)
                    row[phase + '_verified'] = True
                    if phase == 'before':
                        failure = runpasses(module, settings['pipeline'].encode(), machine, options)
                        if failure:
                            error = geterror(failure)
                            message = C.string_at(error).decode()
                            droperror(error)
                            raise RuntimeError(message)
                printed = printmod(module)
                (output / (item['name'] + '.ll')).write_bytes(C.string_at(printed))
                dispose_msg(printed)
                obj, error = V(), V()
                status = emit(machine, module, 1, C.byref(error), C.byref(obj))
                message = C.string_at(error.value).decode() if error.value else ''
                dispose_msg(error)
                if status:
                    raise RuntimeError('object emission: ' + message)
                (output / (item['name'] + '.o')).write_bytes(C.string_at(bufstart(obj), bufsize(obj)))
                bufdrop(obj)
                row.update(valid=True, object_sha256=sha(output / (item['name'] + '.o')))
            finally:
                if module:
                    mod_drop(module)
                ctx_drop(context)
                row['diagnostic_elapsed_ns'] = time.monotonic_ns() - started
                save()
        receipt['status'] = 'Pass'
    except BaseException as error:
        receipt.update(status='Fail', error=repr(error))
        raise
    finally:
        dropopts(options)
        droplayout(layout)
        droptm(machine)
        save()


if __name__ == '__main__':
    main()

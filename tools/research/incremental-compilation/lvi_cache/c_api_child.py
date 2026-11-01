#!/usr/bin/env python3
"""Fresh process loading exactly one absolute LLVM DSO, with explicit C ABI signatures."""
import ctypes as C,hashlib,json,sys
from pathlib import Path
assert len(sys.argv)==5 and sys.argv[4] in ['false','true'];per_pred=sys.argv[4]=='true';libpath,planpath,out=map(Path,sys.argv[1:4]);out.mkdir(exist_ok=False);lib=C.CDLL(str(libpath),mode=C.RTLD_LOCAL);V=C.c_void_p;I=C.c_int;S=C.c_char_p;Z=C.c_size_t;P=C.POINTER(V)
def bind(name,restype,args):
 f=getattr(lib,name);f.restype=restype;f.argtypes=args;return f
ctx_new=bind('LLVMContextCreate',V,[]);ctx_drop=bind('LLVMContextDispose',None,[V]);buffer_new=bind('LLVMCreateMemoryBufferWithMemoryRangeCopy',V,[S,Z,S]);parse=bind('LLVMParseIRInContext',I,[V,V,P,P]);verify=bind('LLVMVerifyModule',I,[V,I,P]);dispose_msg=bind('LLVMDisposeMessage',None,[V]);mod_drop=bind('LLVMDisposeModule',None,[V]);printmod=bind('LLVMPrintModuleToString',V,[V]);gettarget=bind('LLVMGetTargetFromTriple',I,[S,P,P]);newtm=bind('LLVMCreateTargetMachine',V,[V,S,S,S,I,I,I]);droptm=bind('LLVMDisposeTargetMachine',None,[V]);newlayout=bind('LLVMCreateTargetDataLayout',V,[V]);droplayout=bind('LLVMDisposeTargetData',None,[V]);setlayout=bind('LLVMSetModuleDataLayout',None,[V,V]);settarget=bind('LLVMSetTarget',None,[V,S]);newopts=bind('LLVMCreatePassBuilderOptions',V,[]);dropopts=bind('LLVMDisposePassBuilderOptions',None,[V]);runpasses=bind('LLVMRunPasses',V,[V,S,V,V]);geterror=bind('LLVMGetErrorMessage',V,[V]);droperror=bind('LLVMDisposeErrorMessage',None,[V]);emit=bind('LLVMTargetMachineEmitToMemoryBuffer',I,[V,V,I,P,P]);bufstart=bind('LLVMGetBufferStart',V,[V]);bufsize=bind('LLVMGetBufferSize',Z,[V]);bufdrop=bind('LLVMDisposeMemoryBuffer',None,[V]);getversion=bind('LLVMGetVersion',None,[C.POINTER(C.c_uint)]*3)
for name in ['LLVMInitializeX86TargetInfo','LLVMInitializeX86Target','LLVMInitializeX86TargetMC','LLVMInitializeX86AsmPrinter','LLVMInitializeX86AsmParser']:bind(name,None,[])()
class DlInfo(C.Structure):_fields_=[('filename',S),('base',V),('symbol',S),('address',V)]
libc=C.CDLL(None);libc.dladdr.argtypes=[V,C.POINTER(DlInfo)];libc.dladdr.restype=I;info=DlInfo();assert libc.dladdr(C.cast(lib.LLVMRunPasses,V),C.byref(info));actual=Path(info.filename.decode()).resolve();assert actual==libpath.resolve();major=C.c_uint();minor=C.c_uint();patch=C.c_uint();getversion(C.byref(major),C.byref(minor),C.byref(patch));assert [major.value,minor.value,patch.value]==[23,1,4]
maps=[x for x in Path('/proc/self/maps').read_text().splitlines() if 'libLLVM' in x];(out/'loaded-maps.txt').write_text('\n'.join(maps)+'\n');assert {Path(x.split()[-1]).resolve() for x in maps}=={actual}
options_argv=(S*2)(b'lvi-cache-research',b'-lvi-per-pred-ranges='+str(per_pred).lower().encode());bind('LLVMParseCommandLineOptions',None,[I,C.POINTER(S),S])(2,options_argv,b'Isolated LVI cache control')
target=V();msg=V();assert gettarget(b'x86_64-unknown-linux-gnu',C.byref(target),C.byref(msg))==0;machine=newtm(target,b'x86_64-unknown-linux-gnu',b'x86-64',b'',2,2,0);assert machine;layout=newlayout(machine);assert layout;opts=newopts();assert opts
for name in ['VerifyEach','LoopInterleaving','LoopVectorization','SLPVectorization','MergeFunctions']:bind('LLVMPassBuilderOptionsSet'+name,None,[V,I])(opts,1)
rows=[]
for case in json.loads(planpath.read_text()):
 source=Path(case['source']);raw=source.read_bytes();ctx=ctx_new();buf=buffer_new(raw,len(raw),str(source).encode());module=V();err=V();status=parse(ctx,buf,C.byref(module),C.byref(err));assert status==0, C.string_at(err.value) if err.value else status
 settarget(module,b'x86_64-unknown-linux-gnu');setlayout(module,layout)
 for phase in ['before','after']:
  err=V();status=verify(module,2,C.byref(err));message=C.string_at(err.value).decode() if err.value else '';dispose_msg(err);assert status==0,(case['name'],phase,message)
  if phase=='before':
   failure=runpasses(module,case['pipeline'].encode(),machine,opts)
   if failure:
    err=geterror(failure);message=C.string_at(err).decode();droperror(err);raise RuntimeError(message)
 printed=printmod(module);ir=C.string_at(printed);dispose_msg(printed);(out/(case['name']+'.ll')).write_bytes(ir);row={'case':case['name'],'pipeline':case['pipeline'],'source_sha256':hashlib.sha256(raw).hexdigest(),'before_verify':True,'after_verify':True,'ir_sha256':hashlib.sha256(ir).hexdigest()}
 if case.get('emit',False):
  err=V();ob=V();status=emit(machine,module,1,C.byref(err),C.byref(ob));message=C.string_at(err.value).decode() if err.value else '';dispose_msg(err);assert status==0,(case['name'],message);data=C.string_at(bufstart(ob),bufsize(ob));assert data.startswith(b'\x7fELF');(out/(case['name']+'.o')).write_bytes(data);bufdrop(ob);row['object_sha256']=hashlib.sha256(data).hexdigest()
 rows.append(row);mod_drop(module);ctx_drop(ctx)
dropopts(opts);droplayout(layout);droptm(machine)
receipt={'status':'Pass','library':str(actual),'library_sha256':hashlib.sha256(actual.read_bytes()).hexdigest(),'loaded_symbol':info.symbol.decode() if info.symbol else None,'version':[major.value,minor.value,patch.value],'options':{'target':'x86_64-unknown-linux-gnu','cpu':'x86-64','features':'','codegen_optimization':2,'relocation':2,'code_model':0,'per_predecessor_ranges':per_pred,'verify_each':True,'loop_interleaving':True,'loop_vectorization':True,'slp_vectorization':True,'merge_functions':True},'rows':rows};(out/'result.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps({'status':'Pass','cases':len(rows),'library':str(actual)}))

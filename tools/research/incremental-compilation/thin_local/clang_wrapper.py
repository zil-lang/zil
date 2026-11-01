#!/usr/bin/env python3
"""Research-only actual-CGU compiler/linker wrapper. Configuration is explicit."""
import hashlib,json,os,shutil,subprocess,sys,time
from pathlib import Path

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def digest(x):return hashlib.sha256(json.dumps(x,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def main():
 config=Path(os.environ['ZIL_THIN_CONFIG']);c=json.loads(config.read_text());root=Path(c['request']);cache=Path(c['cache']);mode=c['mode'];original=sys.argv[1:]
 def tool(name):
  item=c['tools'][name];p=Path(item['path']);st=p.stat()
  assert st.st_size==item['bytes'] and st.st_mtime_ns==item['mtime_ns'],f'{name} changed during request'
  return str(p)
 clang=tool('clang');lld=tool('lld');tool('zil')
 def invoke(command,label):
  start=time.perf_counter_ns();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
  (root/(label+'.stdout')).write_bytes(r.stdout);(root/(label+'.stderr')).write_bytes(r.stderr)
  receipt={'original_command':original,'actual_command':command,'exit':r.returncode,'elapsed_ns':time.perf_counter_ns()-start,'config_sha256':sha(config)}
  (root/(label+'.command.json')).write_text(json.dumps(receipt,indent=2)+'\n')
  sys.stdout.buffer.write(r.stdout);sys.stderr.buffer.write(r.stderr)
  if r.returncode:raise SystemExit(r.returncode)
 def compile_unit(input_path,output_path,flags):
  input_path=Path(input_path);name=input_path.name
  assert name.startswith('unit-') and name.endswith('.ll'), 'public module.ll is not an actual-CGU capture'
  captured=root/'units'/name
  assert input_path.read_bytes()==captured.read_bytes(), 'actual compiler input differs from preflight capture'
  units=json.loads((root/'units/preflight.json').read_text());unit=int(name.removeprefix('unit-').removesuffix('.ll'))
  assert units[unit]['unit']==unit and units[unit]['machine_preflight_current'] is True
  lines=captured.read_text().splitlines(keepends=True)
  assert lines[0]=='; ZIL verified LLVM module\n' and lines[1].startswith('; verified-fingerprint: ')
  # Exactly one documented diagnostic authority comment is normalized. All
  # code, symbols, attributes, ABI, layout and actual imported bodies remain.
  lines[1]='; verified-fingerprint: current-request-preflight-recorded-separately\n'
  if c['dso_local']:
   remaining=set(units[unit]['dso_candidates'])
   for i,line in enumerate(lines):
    if not line.startswith('define '):continue
    symbol=line.partition(' @')[2].partition('(')[0]
    if symbol in remaining:
     assert not line.startswith(('define internal ','define private ','define available_externally '))
     lines[i]='define dso_local '+line.removeprefix('define ');remaining.remove(symbol)
   assert not remaining, 'structured DSO candidate missing from serialized definition'
  normalized=''.join(lines).encode()
  (root/('unit-'+str(unit)+'.normalized.ll')).write_bytes(normalized)
  compile_flags=[*flags,'-x','ir','-O2','-ffunction-sections','-fdata-sections','-c']
  if mode=='thin':compile_flags+=['-flto=thin']
  material={'schema':2,'mode':mode,'dso_local':c['dso_local'],'input_sha256':hashlib.sha256(normalized).hexdigest(),'flags':compile_flags,'tools':c['tools'],'wrapper_sha256':sha(Path(__file__))}
  key=digest(material);source=cache/'sources'/(key+'.ll');source.parent.mkdir(parents=True,exist_ok=True)
  if source.exists():assert source.read_bytes()==normalized
  else:source.write_bytes(normalized)
  entry=cache/'pre'/key;pending=root/'pending'/key;pending.parent.mkdir(parents=True,exist_ok=True)
  suffix='.bc' if mode=='thin' else '.o';product=entry/suffix.removeprefix('.');meta=entry/'receipt.json';hit=False
  if product.is_file() and meta.is_file():
   try:
    prior=json.loads(meta.read_text())
    if prior.get('material')==material and prior.get('product_sha256')==sha(product):
     shutil.copy2(product,output_path);hit=True
   except (ValueError,OSError):pass
  if not hit:
   command=[clang,*compile_flags,str(source),'-o',str(output_path)]
   invoke(command,name+'.compile')
   if mode=='thin':assert Path(output_path).read_bytes().startswith(b'BC\xc0\xde'), 'expected ThinLTO bitcode'
   pending.mkdir(exist_ok=True);shutil.copy2(output_path,pending/suffix.removeprefix('.'))
   (pending/'receipt.json').write_text(json.dumps({'material':material,'product_sha256':sha(Path(output_path))},indent=2)+'\n')
  shutil.copy2(output_path,root/(name+suffix))
  (root/(name+'.input.json')).write_text(json.dumps({'unit':unit,'raw_sha256':sha(captured),'normalized_sha256':material['input_sha256'],'action_key':key,'pre_cache_hit':hit,'product_sha256':sha(Path(output_path)),'original_command':original,'current_machine_preflight':True,'current_verification_is_in_driver_report':True},indent=2)+'\n')
 def link(objects,output_path,flags):
  symbols=[s for s in (root/'units/exports.txt').read_text().splitlines() if s]
  exports=root/'exports.list';exports.write_text('{\n'+''.join('  '+json.dumps(s)+';\n' for s in symbols)+'};\n')
  command=[clang,*flags,'-O2','-fuse-ld='+lld,*[str(p) for p in objects],'-o',str(output_path),'-Wl,--gc-sections','-Wl,--export-dynamic-symbol-list='+str(exports)]
  post=root/'post-cache';post.mkdir(exist_ok=True)
  if mode=='thin':command+=['-flto=thin','-Wl,--thinlto-cache-dir='+str(post),'-Wl,--thinlto-jobs='+str(c['jobs'])]
  invoke(command,'link')
  products=[{'path':str(p),'sha256':sha(Path(p))} for p in objects]
  (root/'link-inputs.json').write_text(json.dumps({'objects_in_order':products,'exports':symbols,'command':command,'tools':c['tools'],'output_sha256':sha(Path(output_path)),'link_cache_used':False,'post_lto_cache_managed_by_lld':mode=='thin','scope':'Always relink. This invocation record is not a complete system-library link reuse key.'},indent=2)+'\n')
 assert '-o' in original;oi=original.index('-o');out=Path(original[oi+1]);inputs=[Path(x) for i,x in enumerate(original) if i!=oi+1 and x.endswith(('.ll','.o'))]
 flags=[x for x in original if x.startswith('--target=')]
 assert len(flags)==1 and flags[0]=='--target=x86_64-unknown-linux-gnu'
 if '-c' in original:
  assert len(inputs)==1 and inputs[0].suffix=='.ll';compile_unit(inputs[0],out,flags)
 elif any(p.suffix=='.ll' for p in inputs):
  assert len(inputs)==1;obj=root/'single-input.o';compile_unit(inputs[0],obj,flags);link([obj],out,flags)
 else:
  assert inputs and all(p.suffix=='.o' for p in inputs);link(inputs,out,flags)
if __name__=='__main__':main()

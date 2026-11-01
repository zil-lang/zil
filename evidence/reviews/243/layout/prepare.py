from pathlib import Path
import json,hashlib,datetime,subprocess
D=Path('/tmp/review243-layout');cases=[]
IO='extern "C" fn putchar(code:i32)->i32;\nfn emit(code:i32){unsafe{putchar(code)}}\n'
R='resource R{code:i32}\nimpl Drop for R{fn drop(self:Self){emit(.code)}}\n'
def case(name,files,rows,reason,layout=None,context=None):
 p=D/'sources'/name;p.mkdir(parents=True,exist_ok=True)
 for n,t in files.items():(p/n).write_text(t)
 cases.append({'name':name,'files_sha256':{n:hashlib.sha256(t.encode()).hexdigest() for n,t in files.items()},'rows':[{'stdin':s,'stdout':o,'exit':e} for s,o,e in rows],'independent_reason':reason,'layout':layout,'same_process_context_group':context})
case('mixed_i32',{'main.zil':IO+R+'''enum Mix{Empty,Data(i32,manages R,i32)}
fn main()->i32{let e=Mix.Data(65,R{code:66},67);match e{Mix.Empty()=>{return 3},Mix.Data(a,r,z)=>{emit(a);emit(z);return 0}}}
'''},[('','ACB',0)],'Both Copy fields are read separately as A/C; the actual middle R payload closes once as B after the arm returns.',{'payload_offsets':[0,4,8],'payload_size':12,'payload_align':4,'enum_payload_offset':4,'enum_size':16,'enum_align':4,'llvm_enum':'{ i32, [3 x i32] }'})
case('mixed_wide',{'main.zil':IO+'''resource R{wide:i64;code:i32;tail:bool}
impl Drop for R{fn drop(self:Self){if .wide!=4294967296i64{emit(88)}if !.tail{emit(89)}emit(.code)}}
enum Mix{Empty,Data(i32,manages R,i32)}
fn main()->i32{let e=Mix.Data(65,R{wide:4294967296i64,code:66,tail:true},67);match e{Mix.Empty()=>{return 3},Mix.Data(a,r,z)=>{emit(a);emit(z);return 0}}}
'''},[('','ACB',0)],'A and C occupy opposite sides of the aligned resource. R checks its wide/bool fields before emitting B; any X/Y indicates a payload projection error.',{'R_offsets':[0,8,12],'R_size':16,'R_align':8,'payload_offsets':[0,8,24],'payload_size':32,'enum_payload_offset':8,'enum_size':40,'enum_align':8,'llvm_enum':'{ i32, [4 x i64] }'})
case('managed_header',{'main.zil':IO+'''enum Mix{Empty,Data(i32,manages ByteBuffer,i32)}
fn main()->i32{let b=bytes_new();if !bytes_reserve(b,1){return 7}bytes_push(b,90u8);let e=Mix.Data(65,give b,67);match e{Mix.Empty()=>{return 3},Mix.Data(a,bytes,z)=>{emit(a);if bytes_len(bytes)!=1{return 9}let view=bytes_view(bytes);emit(i32(view[0]));emit(z);return 0}}}
'''},[('','AZC',0)],'The middle Managed ByteBuffer is its real three-word header. The read length/data require correct ptr/len/cap offsets; cleanup frees the buffer but prints nothing.',{'header_size':24,'header_align':8,'payload_offsets':[0,8,32],'payload_size':40,'enum_payload_offset':8,'enum_size':48,'enum_align':8,'llvm_enum':'{ i32, [5 x i64] }'})
case('nested_sum',{'main.zil':IO+R+'''enum Inner{None,Some(manages R)}
enum Outer{Empty,Data(i32,Inner,i32)}
fn main()->i32{let e=Outer.Data(65,Inner.Some(R{code:66}),67);match e{Outer.Empty()=>{return 3},Outer.Data(a,Inner.None(),z)=>{return 4},Outer.Data(a,Inner.Some(r),z)=>{emit(a);emit(z);return 0}}}
'''},[('','ACB',0)],'Outer a/z are A/C; recursive selected Inner owns R and closes B. An inactive Inner branch cannot contribute a debt.',{'inner_size':8,'inner_align':4,'outer_payload_offsets':[0,4,12],'outer_payload_size':16,'outer_payload_offset':4,'outer_size':20,'outer_align':4,'llvm_outer':'{ i32, [4 x i32] }'})
case('moved_flags',{'main.zil':IO+R+'''enum E{Some(manages R),None}
fn relay(value:E)->E{return value}
fn main()->i32{var value=E.Some(R{code:65});let later=R{code:76};value=relay(value);emit(90);return 0}
'''},[('','ZLA',0)],'Self transfer publishes into the same lexical shell without early/double cleanup. Z precedes later L, then original shell A; moved temporary conditional drops print nothing.')
case('entry_error_enum',{'main.zil':IO+'extern "C" fn getchar()->i32;\nfn read()->i32{unsafe{return getchar()}}\n'+R+'''enum Fault{Owned(manages R),Empty}
fn main()->Fault!i32{let local=R{code:76};if read()==49{return Fault.Owned(R{code:69})}return 0}
'''},[('0','L',0),('1','LE',1)],'Local L closes in source main. Only the actual error return transfers E to native entry cleanup, then exits1. The inactive error slot would contain zero-tag Owned, so accidental cleanup on success is observable as an unwanted NUL.',{'fault_enum_size':8,'fault_enum_align':4,'error_union_offsets':[0,4,12],'error_union_size':16,'error_union_align':4,'error_tag':'i1','enum_tag':'i32'})
lib=IO+R+'''enum State{Some(manages R),None}
pub package App{state:State;generation:i32}
pub fn make()->App{return App{state:State.Some(R{code:65}),generation:40}}
pub fn increment(value:App)->App{let state=value.state;return App{state:state,generation:value.generation+1}}
pub fn finish(value:App)->i32{return value.generation}
'''
case('opaque_app',{'lib.zil':lib,'main.zil':IO+'''import lib
fn main()->i32{var app=lib.make();app=lib.increment(app);app=lib.increment(app);let generation=lib.finish(app);if generation!=42{return 9}emit(90);return 0}
'''},[('','AZ',0)],'Private generation40 is copied exactly through two self replacements to42. finish consumes App, closes its actual State/R as A, then caller emits Z. The caller has no source field access.',{'state_size':8,'state_align':4,'App_offsets':[0,8],'App_size':12,'App_align':4})
(D/'negative').mkdir(exist_ok=True);(D/'negative/opaque_app_private_read.zil').write_text(IO+'import lib\nfn main()->i32{var app=lib.make();app=lib.increment(app);return app.generation}\n')
left=IO+'''resource Leaf{code:i32}impl Drop for Leaf{fn drop(self:Self){emit(.code)}}
pub enum Choice{Empty,Some(manages Leaf)}
pub fn make()->Choice{return Choice.Some(Leaf{code:65})}
'''
right=IO+'''resource Leaf{code:i32}impl Drop for Leaf{fn drop(self:Self){emit(.code+32)}}
pub enum Choice{Empty,Some(manages Leaf)}
pub fn make()->Choice{return Choice.Some(Leaf{code:66})}
'''
main='import left\nimport right\nfn main()->i32{let a=left.make();let b=right.make();return 0}\n'
case('nominal_same_layout',{'left.zil':left,'right.zil':right,'main.zil':main},[('','bA',0)],'Two different modules deliberately use the same short Enum/resource names and physical layout. Right teardown adds32, so reusing the left resource teardown would emit B instead of b. Locals close right then left.',{'left_enum_size':8,'right_enum_size':8,'align':4})
right_wide=IO+'''resource Leaf{wide:i64;code:i32}impl Drop for Leaf{fn drop(self:Self){if .wide!=4294967296i64{emit(88)}emit(.code+32)}}
pub enum Choice{Empty,Some(manages Leaf)}
pub fn make()->Choice{return Choice.Some(Leaf{wide:4294967296i64,code:66})}
'''
case('nominal_different_layout',{'left.zil':left,'right.zil':right_wide,'main.zil':main},[('','bA',0)],'Equal short names do not identify types or layouts. Right leaf is aligned8 with code at8; its Enum is24 bytes, while left is8. Right b precedes left A.',{'left_enum_size':8,'left_enum_align':4,'right_resource_offsets':[0,8],'right_resource_size':16,'right_enum_size':24,'right_enum_align':8})
context_common='resource R{code:i32}\nenum Choice{Empty,Some(manages R)}\n'
context_main='fn main()->i32{let value=Choice.Some(R{code:0});return 0}\n'
case('context_a',{'main.zil':IO+context_common+'impl Drop for R{fn drop(self:Self){emit(65)}}\n'+context_main},[('','A',0)],'First emission in one process/session has this complete Choice/R type and its actual resource teardown binding.',context='same_complete_type_resource_binding')
case('context_b',{'main.zil':IO+context_common+'fn decoy(value:R){emit(88)}\nimpl Drop for R{fn drop(self:Self){emit(66)}}\n'+context_main},[('','B',0)],'Compile immediately after context_a in the same process/session with the same SourceUnit identity. Complete Type remains equal but the inserted function changes the real teardown FunctionId; no stale plan may call the old binding.',context='same_complete_type_resource_binding')
meta={'reviewer':'/root/ir_defer227','scope':'243 machine/layout/drop-plan supplement only; does not review the author239 IR','state':'FrozenExpectationsBeforeExecution','frozen_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'spec_hash':'c155bd918e73f365de6a7dc210dd9f08bcd807e402921336498631d54eb8cc72','normative_hash':'0e3808d47c1a9e7fcd2aeb06733a3d1ab3ed8cd00def193a193a0e0d55526e66','compiler_executions_so_far':0,'planned_opts':['O0','O2'],'planned_native_executions':sum(len(c['rows']) for c in cases)*2,'allocation_failure_matrix':'NotRun:244 owns independent source/allocation matrix','cases':cases,'negative_source_expectation':{'case':'opaque_app_private_read','phase':'Types','code':'E-FIELD-PRIVATE','verification_runs':0,'executable':None,'lib_source':'sources/opaque_app/lib.zil'},'raw_LIR_expectations':{'positive':'Source-derived independently VerifiedModule lowered normally must emit','mutations':['different_target','changed_resource_teardown','changed_enum_local_type','changed_Unpack_field_mapping','changed_DropEnum_local','copied_verified_fingerprint_with_changed_CFG'],'expected_code':'E-CODEGEN-UNBOUND-LIR','authority':'Each mutant retains copied fingerprint but fails exact capability binding; no generated machine execution counted.'},'illegal_tag_check':'No new source or FFI ability to manufacture an invalid Enum tag is introduced. Check actual emitted trap/unreachable CFG for out-of-range tags before payload access and final cleanup fallthrough; native corruption injection is not claimed as legal ZIL evidence.','expected_outputs_derived_from':'Source evaluation order, declared payload layout/alignment, actual module resources, reverse local initialization cleanup and recursive declared field cleanup; no compiler/native output read to select these expectations.'}
(D/'ORACLE.json').write_text(json.dumps(meta,indent=2)+'\n');print('cases',len(cases),'planned native',meta['planned_native_executions']);print('oracle_sha256',hashlib.sha256((D/'ORACLE.json').read_bytes()).hexdigest())

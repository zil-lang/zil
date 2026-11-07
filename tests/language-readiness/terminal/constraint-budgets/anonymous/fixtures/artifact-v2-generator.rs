use zil_base::{Fingerprint,Limits,ModuleId,NodeId,Span};
use zil_ir::{Constant,ct_artifact::*,ct_wire};
fn main(){
 assert_eq!(CT_ARTIFACT_SCHEMA_VERSION,2);
 let span=Span::default();let t=CtTy::Scalar(CtScalarTy::I32);
 let p=CtArtifactProgram{schema_version:2,target_pointer_width:64,source_fingerprint:Fingerprint::ZERO,
 functions:vec![CtFunction{id:CtFunctionId(0),definition:CtSymbolId{module:ModuleId(0),declaration:NodeId(1),generation_path:None},parameters:vec![],parameter_types:vec![],result_type:t.clone(),locals:vec![],values:vec![t],blocks:vec![CtBlock{id:CtBlockId(0),instructions:vec![CtInstruction{result:Some(CtValueId(0)),kind:CtInstructionKind::Constant(CtConstantId(0)),span}],terminator:CtTerminator::Return(Some(CtValueId(0)))}],entry:CtBlockId(0),span}],entry:CtFunctionId(0),constants:vec![CtConstant::Scalar(Constant::I32(4))],record_schemas:vec![],enum_schemas:vec![],templates:vec![],input_tokens:CtInputTable{invocation_fingerprint:Fingerprint::ZERO,tokens:vec![]},snapshot:CtSnapshotRequest{fingerprint:Fingerprint::ZERO,ready:CtReadyStage::Tokens,target_fingerprint:None,requested_queries:vec![]},budget:CtDeclaredBudget{max_steps:100,max_call_depth:10,max_live_cells:100,max_temporary_bytes:1024,max_generated_nodes:100,max_instances:10,max_analysis_work:1000}};
 let limits=Limits::default();let bytes=ct_wire::encode_program(&p,&limits).unwrap();assert_eq!(ct_wire::decode_program(&bytes,&limits).unwrap(),p);
 std::fs::write("/workspace/zil-344/tests/language-readiness/terminal/constraint-budgets/anonymous/fixtures/artifact-v2-program.wire",bytes).unwrap();
 let v=CtValue::Tuple(vec![CtValue::Scalar(Constant::F64(0x7ff8_0000_0000_0042)),CtValue::Scalar(Constant::U64(u64::MAX))]);let bytes=ct_wire::encode_value(&v,&limits).unwrap();assert_eq!(ct_wire::decode_value(&bytes,&limits).unwrap(),v);
 std::fs::write("/workspace/zil-344/tests/language-readiness/terminal/constraint-budgets/anonymous/fixtures/artifact-v2-value.wire",bytes).unwrap();
}

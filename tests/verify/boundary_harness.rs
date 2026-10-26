//! Executable boundary evidence built against the real zil-verify crate. These
//! are raw-IR/cache mutation tests, not ZIL source or native backend conformance.
use std::{fs, path::Path};
use zil_base::{
    CompileResult, FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, Span,
    TargetProfile,
};
use zil_ir::*;
use zil_verify::{verify_ct_bytes, verify_module_bytes};

#[derive(Default)]
struct Checks {
    executed: usize,
    assertions: usize,
    observations: Vec<String>,
}
impl Checks {
    fn reject<T>(&mut self, result: CompileResult<T>, category: ResultCategory, code: &str) {
        let errors = match result {
            Ok(_) => panic!("expected rejection {code}, got a verified value"),
            Err(errors) => errors,
        };
        assert!(!errors.is_empty());
        assert_eq!(errors[0].phase, Phase::Verify);
        assert_eq!(errors[0].category, category);
        assert_eq!(errors[0].code, code);
        self.executed += 1;
        self.assertions += 4;
        self.observations.push(errors[0].to_json());
    }
    fn report(&self) {
        println!(
            "{{\"status\":\"Pass\",\"evidence_level\":\"RawCandidateBoundary\",\"executed\":{},\"assertions\":{},\"observations\":[{}]}}",
            self.executed,
            self.assertions,
            self.observations.join(",")
        );
    }
}
fn ct() -> CtModule {
    CtModule {
        schema_version: zil_base::SCHEMA_VERSION,
        input_fingerprint: Fingerprint::of(b"boundary raw CT input"),
        target: TargetProfile::host_llvm(),
        instructions: vec![
            CtInstruction {
                result: ValueId(0),
                ty: Type::I32,
                kind: CtInstructionKind::Constant(Constant::I32(20)),
            },
            CtInstruction {
                result: ValueId(1),
                ty: Type::I32,
                kind: CtInstructionKind::Constant(Constant::I32(22)),
            },
            CtInstruction {
                result: ValueId(2),
                ty: Type::I32,
                kind: CtInstructionKind::Binary {
                    op: BinaryOp::Add,
                    left: ValueId(0),
                    right: ValueId(1),
                },
            },
        ],
        result: ValueId(2),
        max_steps: 3,
        dependencies: Vec::new(),
    }
}
fn first_opcode(bytes: &[u8]) -> usize {
    fn skip_text(bytes: &[u8], position: &mut usize) {
        let count =
            u32::from_le_bytes(bytes[*position..*position + 4].try_into().unwrap()) as usize;
        *position += 4 + count;
    }
    // Explicit v1 format offsets, checked against a positive round-trip below.
    let mut position = 8 + 4 + 4 + 32;
    skip_text(bytes, &mut position);
    skip_text(bytes, &mut position);
    position += 2 + 1;
    skip_text(bytes, &mut position);
    skip_text(bytes, &mut position);
    position += 1 + 8 + 4;
    assert_eq!(&bytes[position..position + 4], &[0, 0, 0, 0]);
    position += 4 + 4 + 4;
    assert_eq!(bytes[position], 0);
    position
}
fn ct_boundary(checks: &mut Checks) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let original = ct();
    let bytes = wire::encode_ct(&original, &limits).unwrap();
    let raw = wire::decode_ct(&bytes, &limits).unwrap();
    assert_eq!(raw, original);
    let checked = verify_ct_bytes(&bytes, &target, &limits).unwrap();
    assert_eq!(checked.module(), &original);
    checks.executed += 1;
    checks.assertions += 2;

    let mut forbidden = original.clone();
    forbidden.instructions[2].kind = CtInstructionKind::Forbidden("host-ffi".into());
    checks.reject(
        verify_ct_bytes(
            &wire::encode_ct(&forbidden, &limits).unwrap(),
            &target,
            &limits,
        ),
        ResultCategory::SafeReject,
        "E-CT-FORBIDDEN",
    );
    let mut false_claim = original.clone();
    false_claim.instructions[2].ty = Type::Bool;
    let encoded = wire::encode_ct(&false_claim, &limits).unwrap();
    assert!(wire::decode_ct(&encoded, &limits).is_ok());
    checks.assertions += 1;
    checks.reject(
        verify_ct_bytes(&encoded, &target, &limits),
        ResultCategory::SafeReject,
        "E-CT-TYPE",
    );
    let mut missing_operand = original;
    missing_operand.instructions[2].kind = CtInstructionKind::Unary {
        op: UnaryOp::Negate,
        operand: ValueId(99),
    };
    checks.reject(
        verify_ct_bytes(
            &wire::encode_ct(&missing_operand, &limits).unwrap(),
            &target,
            &limits,
        ),
        ResultCategory::SafeReject,
        "E-CT-SSA",
    );

    let marker = bytes.windows(4).rposition(|part| part == b"CERT").unwrap();
    checks.reject(
        verify_ct_bytes(&bytes[..marker], &target, &limits),
        ResultCategory::SafeReject,
        "E-CTIR-CERTIFICATE",
    );
    let mut no_claims = bytes.clone();
    no_claims[marker + 4..marker + 8].copy_from_slice(&0u32.to_le_bytes());
    checks.reject(
        verify_ct_bytes(&no_claims, &target, &limits),
        ResultCategory::SafeReject,
        "E-CTIR-CERTIFICATE",
    );
    let mut unknown = bytes.clone();
    unknown[first_opcode(&bytes)] = 201;
    checks.reject(
        verify_ct_bytes(&unknown, &target, &limits),
        ResultCategory::SafeReject,
        "E-CTIR-OPCODE",
    );
    let mut version = bytes.clone();
    version[8..12].copy_from_slice(&999u32.to_le_bytes());
    checks.reject(
        verify_ct_bytes(&version, &target, &limits),
        ResultCategory::SafeReject,
        "E-WIRE-VERSION",
    );
    checks.reject(
        verify_ct_bytes(&bytes[..bytes.len() - 1], &target, &limits),
        ResultCategory::SafeReject,
        "E-WIRE-TRUNCATED",
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    checks.reject(
        verify_ct_bytes(&trailing, &target, &limits),
        ResultCategory::SafeReject,
        "E-WIRE-TRAILING",
    );

    // These assertions check the quota mechanism itself. They are not a claim
    // that an Unsupported/ResourceLimit source compilation passed conformance.
    let limited = Limits {
        max_source_bytes: bytes.len() - 1,
        ..limits.clone()
    };
    checks.reject(
        verify_ct_bytes(&bytes, &target, &limited),
        ResultCategory::ResourceLimit,
        "E-RESOURCE-LIMIT",
    );
    let limited = Limits {
        max_ct_steps: 2,
        ..limits.clone()
    };
    checks.reject(
        verify_ct_bytes(&bytes, &target, &limited),
        ResultCategory::ResourceLimit,
        "E-RESOURCE-LIMIT",
    );
    let count_offset = first_opcode(&bytes) - 8;
    let mut enormous = bytes;
    enormous[count_offset..count_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    checks.reject(
        verify_ct_bytes(&enormous, &target, &limits),
        ResultCategory::ResourceLimit,
        "E-RESOURCE-LIMIT",
    );
}
fn candidate() -> (TypedModule, CandidatePlan) {
    let span = Span::new(FileId(0), 0, 1);
    let module = TypedModule {
        resources: Vec::new(),
        closures: vec![],
        schema_version: zil_base::SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"boundary candidate"),
        target: TargetProfile::host_llvm(),
        functions: vec![TypedFunction {
            result_mode: zil_ir::PassingMode::Infer,
            environment: None,
            callable_return_contract: None,
            callable_contract: None,
            foreign: None,
            id: FunctionId(0),
            name: "answer".into(),
            parameters: vec![],
            return_type: Type::I32,
            locals: vec![LocalDecl {
                id: LocalId(0),
                name: "x".into(),
                ty: Type::I32,
                mutable: false,
                span,
            }],
            values: vec![Type::I32, Type::I32],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction {
                        result: Some(ValueId(0)),
                        kind: InstructionKind::Constant(Constant::I32(42)),
                        span,
                    },
                    Instruction {
                        result: None,
                        kind: InstructionKind::Store {
                            local: LocalId(0),
                            value: ValueId(0),
                        },
                        span,
                    },
                    Instruction {
                        result: Some(ValueId(1)),
                        kind: InstructionKind::Load(LocalId(0)),
                        span,
                    },
                ],
                terminator: Terminator::Return(Some(ValueId(1))),
            }],
            entry: BlockId(0),
            span,
        }],
        unsupported: vec![],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions.push(FunctionPlan {
        function: FunctionId(0),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![LocalId(0)],
        }],
        // Scalar SSA values have no address roots, with one row per ValueId.
        holds: vec![
            ValueHold {
                value: ValueId(0),
                roots: vec![],
            },
            ValueHold {
                value: ValueId(1),
                roots: vec![],
            },
        ],
    });
    (module, plan)
}
fn cache_boundary(checks: &mut Checks, directory: &Path) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let (module, mut plan) = candidate();
    let path = directory.join("raw-candidate.cache");
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(&path, &bytes).unwrap();
    let recovered = fs::read(&path).unwrap();
    assert_eq!(Fingerprint::of(&recovered), Fingerprint::of(&bytes));
    let checked = verify_module_bytes(&recovered, &target, &limits).unwrap();
    assert_eq!(checked.module(), &module);
    assert_eq!(checked.plan(), &plan);
    assert_eq!(checked.fingerprint(), module.fingerprint());
    let mut detached = checked.module().clone();
    detached.functions.clear();
    assert_eq!(checked.module().functions.len(), 1);
    checks.executed += 1;
    checks.assertions += 5;

    plan.functions[0].blocks[0].initialized_in.push(LocalId(0));
    let false_claim = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    // A valid digest for malicious cache bytes still does not certify a plan.
    let stored_digest = Fingerprint::of(&false_claim);
    fs::write(&path, &false_claim).unwrap();
    let recovered = fs::read(&path).unwrap();
    assert_eq!(Fingerprint::of(&recovered), stored_digest);
    assert!(wire::decode_module_candidate(&recovered, &limits).is_ok());
    checks.assertions += 2;
    checks.reject(
        verify_module_bytes(&recovered, &target, &limits),
        ResultCategory::SafeReject,
        "E-VERIFY-INITIALIZATION-CLAIM",
    );
    plan.functions[0].blocks[0].initialized_in.clear();
    plan.module_fingerprint = Fingerprint::ZERO;
    checks.reject(
        verify_module_bytes(
            &wire::encode_module_candidate(&module, &plan, &limits).unwrap(),
            &target,
            &limits,
        ),
        ResultCategory::SafeReject,
        "E-VERIFY-PLAN-IDENTITY",
    );
    let mut wrong_target = module.clone();
    wrong_target.target.triple = "unselected-triple".into();
    checks.reject(
        verify_module_bytes(
            &wire::encode_module_candidate(&wrong_target, &plan, &limits).unwrap(),
            &target,
            &limits,
        ),
        ResultCategory::SafeReject,
        "E-VERIFY-TARGET",
    );
    fs::remove_file(path).unwrap();
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut checks = Checks::default();
    match args.get(1).map(String::as_str) {
        Some("ct") => ct_boundary(&mut checks),
        Some("isolation") => cache_boundary(
            &mut checks,
            Path::new(args.get(2).expect("temporary directory")),
        ),
        _ => panic!("expected ct or isolation boundary mode"),
    }
    assert!(checks.executed > 0 && checks.assertions > 0);
    checks.report();
}

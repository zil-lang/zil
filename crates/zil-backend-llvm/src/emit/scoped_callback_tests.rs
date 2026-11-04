//! Raw typed-IR model control. This checks LLVM shape before the independent
//! verifier accepts WithForeignCallback; it is not source or Verified evidence.
use super::*;
use std::io::Write;
use std::process::{Command, Stdio};

fn instruction(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: Span::default(),
    }
}

fn local(id: u32, ty: Type) -> zil_ir::LocalDecl {
    zil_ir::LocalDecl {
        id: LocalId(id),
        name: format!("local{id}"),
        ty,
        mutable: true,
        span: Span::default(),
    }
}

fn function(id: u32, foreign: Option<&str>) -> TypedFunction {
    TypedFunction {
        call_kind: zil_ir::CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(id),
        name: format!("test{id}"),
        foreign: foreign.map(|symbol| zil_ir::ForeignDeclaration {
            abi: "C".into(),
            symbol: symbol.into(),
            noescape_parameters: vec![],
            static_callbacks: true,
            scoped_callback: None,
        }),
        environment: None,
        parameters: vec![],
        return_type: Type::Unit,
        result_mode: zil_ir::PassingMode::Infer,
        locals: vec![],
        values: vec![],
        blocks: vec![],
        entry: zil_ir::BlockId(0),
        span: Span::default(),
    }
}

#[test]
fn scoped_helper_keeps_source_successors_and_verifies_as_llvm() {
    let f = Type::Closure(ClosureId(0));
    let body = Type::Closure(ClosureId(1));
    let pair = Type::Tuple(vec![Type::Bool, Type::I32]);
    let source = TypedFunction {
        call_kind: zil_ir::CallKind::Direct,
        reference_return_bound: None,
        parameters: vec![LocalId(0), LocalId(1)],
        locals: vec![
            local(0, f.clone()),
            local(1, body.clone()),
            local(2, Type::I32),
        ],
        return_type: Type::I32,
        values: vec![
            Type::Address {
                inner: Box::new(f),
                mutable: true,
            },
            body,
            pair,
            Type::I32,
            Type::I32,
            Type::I32,
            Type::Bool,
            Type::I32,
            Type::Bool,
        ],
        blocks: vec![
            zil_ir::BasicBlock {
                id: zil_ir::BlockId(0),
                instructions: vec![
                    instruction(
                        Some(0),
                        InstructionKind::BorrowLocalMut { local: LocalId(0) },
                    ),
                    instruction(Some(1), InstructionKind::Load(LocalId(1))),
                    instruction(
                        Some(2),
                        InstructionKind::WithForeignCallback {
                            register: FunctionId(3),
                            close: FunctionId(4),
                            callback: ValueId(0),
                            body: ValueId(1),
                            region: 0,
                        },
                    ),
                    instruction(
                        Some(3),
                        InstructionKind::ExtractField {
                            aggregate: ValueId(2),
                            index: 1,
                        },
                    ),
                    instruction(Some(4), InstructionKind::Constant(Constant::I32(1))),
                    instruction(
                        Some(5),
                        InstructionKind::Binary {
                            op: BinaryOp::Add,
                            left: ValueId(3),
                            right: ValueId(4),
                        },
                    ),
                    instruction(Some(6), InstructionKind::Constant(Constant::Bool(true))),
                ],
                terminator: Terminator::Branch {
                    condition: ValueId(6),
                    then_block: zil_ir::BlockId(1),
                    else_block: zil_ir::BlockId(2),
                },
            },
            zil_ir::BasicBlock {
                id: zil_ir::BlockId(1),
                instructions: vec![instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(2),
                        value: ValueId(5),
                    },
                )],
                terminator: Terminator::Jump(zil_ir::BlockId(3)),
            },
            zil_ir::BasicBlock {
                id: zil_ir::BlockId(2),
                instructions: vec![instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(2),
                        value: ValueId(4),
                    },
                )],
                terminator: Terminator::Jump(zil_ir::BlockId(3)),
            },
            zil_ir::BasicBlock {
                id: zil_ir::BlockId(3),
                instructions: vec![
                    instruction(Some(7), InstructionKind::Load(LocalId(2))),
                    instruction(Some(8), InstructionKind::Constant(Constant::Bool(false))),
                ],
                terminator: Terminator::Branch {
                    condition: ValueId(8),
                    then_block: zil_ir::BlockId(0),
                    else_block: zil_ir::BlockId(4),
                },
            },
            zil_ir::BasicBlock {
                id: zil_ir::BlockId(4),
                instructions: vec![],
                terminator: Terminator::Return(Some(ValueId(7))),
            },
        ],
        ..function(0, None)
    };
    let register = function(3, Some("test_register"));
    let close = function(4, Some("test_close"));
    let closures = [
        ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(1),
            captures: vec![],
            parameters: vec![Type::I32],
            result: Type::I32,
            kind: CallableKind::FnMut,
            lending_captures: vec![],
        },
        ClosureDefinition {
            id: ClosureId(1),
            function: FunctionId(2),
            captures: vec![],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::FnOnce,
            lending_captures: vec![],
        },
    ];
    let definitions = [
        source,
        function(1, None),
        function(2, None),
        register,
        close,
    ];
    let functions = definitions
        .iter()
        .map(|f| (f.id, f))
        .collect::<BTreeMap<_, _>>();
    let closures = closures
        .iter()
        .map(|c| (c.id, c))
        .collect::<BTreeMap<_, _>>();
    let target = TargetProfile::host_llvm();
    let mut intrinsics = BTreeSet::new();
    let mut code = m::Code::from(vec![
        m::Event::NamedType("zil_env_0".into(), vec![]),
        m::Event::NamedType("zil_env_1".into(), vec![]),
        m::Event::Declare(m::Function {
            name: "llvm.trap".into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(m::Ty::Void, vec![]),
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Declare(m::Function {
            name: "test_register".into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(
                m::Ty::Ptr,
                vec![m::Param::unnamed(m::Ty::Ptr), m::Param::unnamed(m::Ty::Ptr)],
            ),
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Declare(m::Function {
            name: "test_close".into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(m::Ty::Void, vec![m::Param::unnamed(m::Ty::Ptr)]),
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Declare(m::Function {
            name: "zil.fn.1".into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(
                m::Ty::Int(32),
                vec![
                    m::Param::unnamed(m::Ty::Ptr),
                    m::Param::unnamed(m::Ty::Int(32)),
                ],
            ),
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Declare(m::Function {
            name: "zil.fn.2".into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(m::Ty::Int(32), vec![m::Param::unnamed(m::Ty::Ptr)]),
            attrs: vec![],
            blocks: None,
        }),
    ]);
    code.append(
        FunctionEmitter {
            function: &definitions[0],
            target: &target,
            functions: &functions,
            scalar_summaries: None,
            closures: &closures,
            resources: &[],
            intrinsics: &mut intrinsics,
            text: m::Code::new(),
            temporary: 0,
            continuation: 0,
            partial_packages: BTreeSet::new(),
            safe_divisors: None,
            copy_values: &BTreeMap::new(),
            abi_plans: &large_copy_abi::Plans::default(),
            unsupported_large_copy: false,
        }
        .emit()
        .expect("raw machine lowering"),
    );
    let mut module = m::Module::from_code(target.triple, "raw-callback-llvm-shape".into(), code)
        .expect("machine model");
    for item in &mut module.items {
        if let m::Item::Function(machine) = item
            && machine.name == "zil.fn.0"
        {
            scalar_locals::promote(&definitions[0], machine);
        }
    }
    let ir = module.to_text();
    assert!(ir.contains("call { i1, i32 } @zil.scoped.with.0.2"));
    assert!(ir.contains("br i1 %v.6, label %bb.1, label %bb.2"));
    assert!(ir.contains("bb.3:"));
    assert!(ir.contains("br i1 %v.8, label %bb.0, label %bb.4"));
    assert!(ir.contains("phi { i1, i32 }"));
    let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
    let object = std::env::temp_dir().join(format!("zil-scoped-shape-{}.o", std::process::id()));
    let mut child = Command::new(clang)
        .args(["-x", "ir", "-c", "-o"])
        .arg(&object)
        .arg("-")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start clang");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(ir.as_bytes())
        .expect("send IR");
    let output = child.wait_with_output().expect("wait for clang");
    assert!(
        output.status.success(),
        "LLVM rejected raw machine IR: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_file(object).expect("remove raw LLVM control object");
}

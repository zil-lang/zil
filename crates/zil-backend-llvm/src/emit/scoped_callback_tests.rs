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
        dyn_instance: None,
        retained_return_sources: None,
        storage_life: None,
        call_kind: zil_ir::CallKind::Direct,
        resume_bound: zil_ir::TaskResumeBound::Local,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contracts: Vec::new(),
        id: FunctionId(id),
        name: format!("test{id}"),
        foreign: foreign.map(|symbol| zil_ir::ForeignDeclaration {
            abi: "C".into(),
            symbol: symbol.into(),
            noescape_parameters: vec![],
            static_callbacks: true,
            thread_driver_transfer: false,
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
        dyn_instance: None,
        retained_return_sources: None,
        call_kind: zil_ir::CallKind::Direct,
        resume_bound: zil_ir::TaskResumeBound::Local,
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
    // The actual directory must describe the same entries as the machine
    // helper calls, rather than relying on separately hard-coded prototypes.
    let callback_entry=TypedFunction {
        environment:Some(ClosureId(0)),parameters:vec![LocalId(0)],
        locals:vec![local(0,Type::I32)],return_type:Type::I32,values:vec![Type::I32],
        blocks:vec![zil_ir::BasicBlock{id:zil_ir::BlockId(0),instructions:vec![instruction(Some(0),InstructionKind::Load(LocalId(0)))],terminator:Terminator::Return(Some(ValueId(0)))}],
        ..function(1,None)
    };
    let body_entry=TypedFunction {
        environment:Some(ClosureId(1)),return_type:Type::I32,values:vec![Type::I32],
        blocks:vec![zil_ir::BasicBlock{id:zil_ir::BlockId(0),instructions:vec![instruction(Some(0),InstructionKind::Constant(Constant::I32(0)))],terminator:Terminator::Return(Some(ValueId(0)))}],
        ..function(2,None)
    };
    let register=TypedFunction {
        parameters:vec![LocalId(0),LocalId(1)],
        locals:vec![local(0,Type::ForeignCallback{parameters:vec![Type::ForeignHandle,Type::I32],result:Box::new(Type::I32)}),local(1,Type::ForeignHandle)],
        return_type:Type::ForeignHandle,..function(3,Some("test_register"))
    };
    let close=TypedFunction {
        parameters:vec![LocalId(0)],locals:vec![local(0,Type::ForeignHandle)],
        ..function(4,Some("test_close"))
    };
    let closures = [
        ClosureDefinition {
            storage_life: None,
            id: ClosureId(0),
            function: FunctionId(1),
            captures: vec![],
            parameters: vec![Type::I32],
            result: Type::I32,
            kind: CallableKind::FnMut,
            lending_captures: vec![],
        },
        ClosureDefinition {
            storage_life: None,
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
        callback_entry,
        body_entry,
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
    let closure_definitions = closures.values().map(|c| (*c).clone()).collect::<Vec<_>>();
    let dyn_module = no_dyn_test_module(target.clone(), &definitions, &closure_definitions, &[]);
    let layouts=&LayoutContext::for_test_module(&dyn_module,&zil_base::Limits::default()).unwrap();
    let mut intrinsics = BTreeSet::new();
    let mut code = m::Code::from(vec![
        m::Event::NamedType("zil_env_0".into(), layouts.environment_fields(ClosureId(0)).unwrap()),
        m::Event::NamedType("zil_env_1".into(), layouts.environment_fields(ClosureId(1)).unwrap()),
        m::Event::Declare(m::Function {
            name: "llvm.trap".into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(m::Ty::Void, vec![]),
            attrs: vec![],
            blocks: None,
        }),
    ]);
    for actual in definitions.iter().filter(|actual|actual.foreign.is_some()) {
        code.append(foreign::declaration(layouts,actual,&target).expect("actual foreign model signature"));
    }
    for actual in definitions.iter().filter(|actual|actual.foreign.is_none()) {
    code.append(
        FunctionEmitter {
            layouts,
            dyn_module: &dyn_module,
            task_ownership: None,
            current_instruction: None,
            vacated_pollable_drops: BTreeSet::new(),
            function: actual,
            target: &target,
            functions: &functions,
            scalar_summaries: None,
            closures: &closures,
            resources: &[],
            intrinsics: &mut intrinsics,
            foreign_adapters: &mut BTreeSet::new(),
            machine_meter: &tasks::Meter::new(usize::MAX),
            text: m::Code::new(),
            temporary: 0,
            continuation: 0,
            partial_packages: BTreeSet::new(),
            replacements: BTreeMap::new(),
            partial_sums: BTreeMap::new(),
            safe_divisors: None,
            copy_values: &BTreeMap::new(),
            abi_plans: &large_copy_abi::Plans::default(),
            unsupported_large_copy: false,
        }
        .emit()
        .expect("raw machine lowering"),
    );
    }
    for(operation,bits)in intrinsics {
        code.push(m::Event::Declare(m::Function{name:format!("llvm.{operation}.with.overflow.i{bits}"),linkage:m::Linkage::External,
            signature:m::Signature::new(m::Ty::Struct(vec![m::Ty::Int(bits),m::Ty::Int(1)]),vec![m::Param::unnamed(m::Ty::Int(bits)),m::Param::unnamed(m::Ty::Int(bits))]),attrs:vec![],blocks:None}));
    }
    let mut module = m::Module::from_code(target.triple, "raw-callback-llvm-shape".into(), code)
        .expect("machine model");
    for item in &mut module.items {
        if let m::Item::Function(machine) = item
            && machine.name == "zil.fn.0"
        {
            scalar_locals::promote(&definitions[0], machine);
        }
    }
    module.validate_complete(|_|Ok(())).expect("complete raw callback machine model");
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

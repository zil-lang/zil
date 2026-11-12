//! Backend machine-model controls, not a source/Verified language acceptance.
use super::*;
use std::{
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn instruction(id: u32, kind: InstructionKind) -> Instruction {
    Instruction {
        result: Some(ValueId(id)),
        kind,
        span: Span::default(),
    }
}
fn function(
    id: u32,
    values: Vec<Type>,
    instructions: Vec<Instruction>,
    returned: u32,
) -> TypedFunction {
    TypedFunction {
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
        foreign: None,
        environment: None,
        parameters: vec![],
        return_type: Type::I32,
        result_mode: zil_ir::PassingMode::Infer,
        locals: vec![],
        values,
        blocks: vec![zil_ir::BasicBlock {
            id: zil_ir::BlockId(0),
            instructions,
            terminator: Terminator::Return(Some(ValueId(returned))),
        }],
        entry: zil_ir::BlockId(0),
        span: Span::default(),
    }
}
fn model() -> (m::Module, LirModule) {
    let closure = ClosureId(0);
    let mut parent = function(
        0,
        vec![
            Type::Closure(closure),
            Type::I32,
            Type::Tuple(vec![Type::Bool, Type::I32, Type::I32]),
            Type::Bool,
            Type::I32,
            Type::I32,
            Type::I32,
            Type::I32,
            Type::I32,
            Type::I32,
        ],
        vec![
            instruction(
                0,
                InstructionKind::MakeClosure {
                    closure,
                    captures: vec![],
                },
            ),
            instruction(1, InstructionKind::Constant(Constant::I32(5))),
            instruction(
                2,
                InstructionKind::WithScopedThread {
                    worker: FunctionId(1),
                    argument: ValueId(1),
                    body: ValueId(0),
                },
            ),
            instruction(
                3,
                InstructionKind::ExtractField {
                    aggregate: ValueId(2),
                    index: 0,
                },
            ),
            instruction(
                4,
                InstructionKind::ExtractField {
                    aggregate: ValueId(2),
                    index: 1,
                },
            ),
            instruction(
                5,
                InstructionKind::ExtractField {
                    aggregate: ValueId(2),
                    index: 2,
                },
            ),
            instruction(6, InstructionKind::Constant(Constant::I32(100))),
            instruction(
                7,
                InstructionKind::Binary {
                    op: BinaryOp::Multiply,
                    left: ValueId(4),
                    right: ValueId(6),
                },
            ),
            instruction(
                8,
                InstructionKind::Binary {
                    op: BinaryOp::Add,
                    left: ValueId(7),
                    right: ValueId(5),
                },
            ),
        ],
        8,
    );
    parent.blocks[0].terminator = Terminator::Branch {
        condition: ValueId(3),
        then_block: zil_ir::BlockId(1),
        else_block: zil_ir::BlockId(2),
    };
    parent.blocks.push(zil_ir::BasicBlock {
        id: zil_ir::BlockId(1),
        instructions: vec![],
        terminator: Terminator::Return(Some(ValueId(8))),
    });
    parent.blocks.push(zil_ir::BasicBlock {
        id: zil_ir::BlockId(2),
        instructions: vec![instruction(9, InstructionKind::Constant(Constant::I32(-1)))],
        terminator: Terminator::Return(Some(ValueId(9))),
    });
    let mut worker = function(
        1,
        vec![Type::I32; 5],
        vec![
            instruction(0, InstructionKind::Load(LocalId(0))),
            instruction(1, InstructionKind::Constant(Constant::I32(2))),
            instruction(
                2,
                InstructionKind::Binary {
                    op: BinaryOp::Multiply,
                    left: ValueId(0),
                    right: ValueId(1),
                },
            ),
            instruction(3, InstructionKind::Constant(Constant::I32(1))),
            instruction(
                4,
                InstructionKind::Binary {
                    op: BinaryOp::Add,
                    left: ValueId(2),
                    right: ValueId(3),
                },
            ),
        ],
        4,
    );
    worker.parameters.push(LocalId(0));
    worker.locals.push(zil_ir::LocalDecl {
        id: LocalId(0),
        name: "payload".into(),
        ty: Type::I32,
        mutable: false,
        span: Span::default(),
    });
    let mut body = function(
        2,
        vec![Type::I32],
        vec![instruction(
            0,
            InstructionKind::ForeignCall {
                function: FunctionId(3),
                arguments: vec![],
                region: 0,
            },
        )],
        0,
    );
    body.environment = Some(closure);
    let mut observer = function(3, vec![], vec![], 0);
    observer.blocks.clear();
    observer.foreign = Some(zil_ir::ForeignDeclaration {
        abi: "C".into(),
        symbol: "body_observe".into(),
        noescape_parameters: vec![],
        static_callbacks: false,
        thread_driver_transfer: false,
        scoped_callback: None,
    });
    let lir = LirModule {
        schema_version: SCHEMA_VERSION,
        module_id: zil_base::ModuleId(0),
        verified_fingerprint: zil_base::Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![parent, worker, body, observer],
        closures: vec![ClosureDefinition {
            storage_life: None,
            id: closure,
            function: FunctionId(2),
            captures: vec![],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::FnOnce,
            lending_captures: vec![],
        }],
        resources: vec![],
        layouts: vec![],
    };
    let functions = lir
        .functions
        .iter()
        .map(|f| (f.id, f))
        .collect::<BTreeMap<_, _>>();
    let closures = lir
        .closures
        .iter()
        .map(|c| (c.id, c))
        .collect::<BTreeMap<_, _>>();
    let mut code = scoped_thread::runtime(&lir.as_view()).unwrap();
    code.push(m::Event::NamedType("zil_env_0".into(), vec![]));
    for (name, result, parameters) in [
        ("llvm.trap", m::Ty::Void, vec![]),
        ("body_observe", m::Ty::Int(32), vec![]),
    ] {
        code.push(m::Event::Declare(m::Function {
            name: name.into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(result, parameters),
            attrs: vec![],
            blocks: None,
        }));
    }
    let mut intrinsics = BTreeSet::new();
    let dyn_module = no_dyn_test_module(
        lir.target.clone(),
        &lir.functions,
        &lir.closures,
        &lir.resources,
    );
    for f in lir.functions.iter().filter(|f| f.foreign.is_none()) {
        code.append(
            FunctionEmitter {
                dyn_module: &dyn_module,
                task_ownership: None,
                vacated_pollable_drops: BTreeSet::new(),
                function: f,
                target: &lir.target,
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
            .unwrap(),
        );
    }
    for (operation, bits) in intrinsics {
        code.push(m::Event::Declare(m::Function {
            name: format!("llvm.{operation}.with.overflow.i{bits}"),
            linkage: m::Linkage::External,
            signature: m::Signature::new(
                m::Ty::Struct(vec![m::Ty::Int(bits), m::Ty::Int(1)]),
                vec![
                    m::Param::unnamed(m::Ty::Int(bits)),
                    m::Param::unnamed(m::Ty::Int(bits)),
                ],
            ),
            attrs: vec![],
            blocks: None,
        }));
    }
    (
        m::Module::from_code(
            lir.target.triple.clone(),
            "thread-backend-model".into(),
            code,
        )
        .unwrap(),
        lir,
    )
}

#[test]
fn scoped_thread_partition_preserves_private_context_and_worker_dependency() {
    let (module, mut lir) = model();
    for function in &mut lir.functions {
        function.span.file = zil_base::FileId(function.id.0);
    }
    let plan = crate::machine_cgu::partition(module, &lir, crate::EntryPoint::None, 4);
    assert!(
        plan.units.len() >= 2,
        "worker and parent use different source files"
    );
    let mut worker_definitions = 0;
    let mut scoped_definitions = 0;
    for unit in &plan.units {
        zil_llvm_api::validate_module(unit).unwrap();
        let definitions = unit
            .items
            .iter()
            .filter_map(|item| match item {
                m::Item::Function(function) if function.blocks.is_some() => {
                    Some(function.name.as_str())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        worker_definitions += usize::from(definitions.contains("zil.fn.1"));
        if definitions.contains("zil.thread.scope.0.2") {
            scoped_definitions += 1;
            assert!(definitions.contains("zil.fn.0"));
            assert!(definitions.contains("zil.thread.start.0.2"));
            assert!(
                unit.items
                    .iter()
                    .any(|item| matches!(item, m::Item::Function(f) if f.name == "zil.fn.1"))
            );
        }
    }
    assert_eq!(worker_definitions, 1);
    assert_eq!(scoped_definitions, 1);
}

#[test]
fn scoped_thread_runtime_binds_target_and_existing_foreign_abi() {
    let (_, mut lir) = model();
    lir.target.triple = "x86_64-pc-windows-msvc".into();
    let errors = scoped_thread::runtime(&lir.as_view()).unwrap_err();
    assert_eq!(errors[0].category, zil_base::ResultCategory::Unsupported);
    let (_, mut lir) = model();
    let declaration = lir.functions.last_mut().unwrap();
    declaration.foreign.as_mut().unwrap().symbol = "abort".into();
    let errors = scoped_thread::runtime(&lir.as_view()).unwrap_err();
    assert_eq!(errors[0].code, "E-CODEGEN-FOREIGN");
    lir.functions.last_mut().unwrap().return_type = Type::Unit;
    let declarations = scoped_thread::runtime(&lir.as_view())
        .unwrap()
        .into_items()
        .unwrap();
    assert_eq!(declarations.len(), 2);
    assert!(
        !declarations
            .iter()
            .any(|item| matches!(item, m::Item::Function(f) if f.name == "abort"))
    );
    lir.target.triple = "x86_64-pc-windows-msvc".into();
    for block in &mut lir.functions[0].blocks {
        block
            .instructions
            .retain(|i| !matches!(i.kind, InstructionKind::WithScopedThread { .. }));
    }
    assert!(
        scoped_thread::runtime(&lir.as_view())
            .unwrap()
            .into_items()
            .unwrap()
            .is_empty()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn scoped_thread_model_runs_real_thread_and_create_join_failures() {
    let (module, lir) = model();
    let ir = module.to_text();
    assert!(ir.contains("pthread_create"));
    assert!(ir.contains("pthread_join"));
    assert!(
        ir.find("thread.join.success:").unwrap() < ir.find("%thread.worker.result = load").unwrap()
    );
    assert!(ir.contains("call void @abort()"));
    let root = std::env::temp_dir().join(format!("zil-thread-model328-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let input = root.join("module.ll");
    fs::write(&input, &ir).unwrap();
    let observer = root.join("observer.c");
    fs::write(&observer, r#"
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
extern int32_t entry(void) __asm__("zil.fn.0");
extern int __real_pthread_create(pthread_t*,const pthread_attr_t*,void*(*)(void*),void*);
extern int __real_pthread_join(pthread_t,void**);
static int mode,bodies,starts,joins;static pthread_t parent;
static void*(*start_fn)(void*);static void *start_arg;
static void check(int x){if(!x)_Exit(99);}
static void *start(void *unused){(void)unused;check(!pthread_equal(parent,pthread_self()));return start_fn(start_arg);}
int __wrap_pthread_create(pthread_t*t,const pthread_attr_t*a,void*(*f)(void*),void*p){
parent=pthread_self();starts++;if(mode==1)return 11;start_fn=f;start_arg=p;return __real_pthread_create(t,a,start,0);}
int32_t body_observe(void){check(pthread_equal(parent,pthread_self()));check(++bodies==1);return 17;}
int __wrap_pthread_join(pthread_t t,void**p){check(bodies==1);joins++;if(mode==2)return 22;return __real_pthread_join(t,p);}
int main(int argc,char**argv){check(argc==2);mode=argv[1][0]-'0';int n=entry();check(starts==1);check(mode==1?n==-1&&bodies==0&&joins==0:n==1117&&bodies==1&&joins==1);return 0;}
"#).unwrap();
    let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
    for optimized in [false, true] {
        let optimization = if optimized { "-O2" } else { "-O0" };
        let mut objects = Vec::with_capacity(2);
        objects.push(("clang", input.clone()));
        #[cfg(feature = "llvm-api")]
        {
            let object = zil_llvm_api::compile_module(
                &module,
                &zil_llvm_api::Config {
                    triple: &lir.target.triple,
                    optimization: if optimized {
                        zil_llvm_api::Optimization::O2
                    } else {
                        zil_llvm_api::Optimization::O0
                    },
                },
                &|| false,
            )
            .unwrap();
            assert_eq!(object.receipt.input_kind, "builder");
            assert_eq!(object.receipt.parse_ns, 0);
            assert!(object.receipt.verify_before_after);
            let path = root.join(format!("api-{optimization}.o"));
            fs::write(&path, object.bytes).unwrap();
            objects.push(("llvm-api", path));
        }
        for (backend, object) in objects {
            let exe = root.join(format!("{backend}-{optimization}"));
            let output = Command::new(&clang)
                .arg(optimization)
                .arg(&object)
                .arg(&observer)
                .args([
                    "-pthread",
                    "-Wl,--wrap=pthread_create",
                    "-Wl,--wrap=pthread_join",
                    "-o",
                ])
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            for mode in ["0", "1", "2"] {
                let mut child = Command::new("sh")
                    .args(["-c", "ulimit -c 0; exec \"$@\"", "thread-test"])
                    .arg(&exe)
                    .arg(mode)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while child.try_wait().unwrap().is_none() {
                    if Instant::now() >= deadline {
                        child.kill().unwrap();
                        let _ = child.wait();
                        panic!("scoped thread model timed out: {backend}/{optimization}/{mode}");
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                let output = child.wait_with_output().unwrap();
                if mode == "2" {
                    use std::os::unix::process::ExitStatusExt;
                    assert_eq!(output.status.signal(), Some(6), "{output:?}");
                } else {
                    assert!(
                        output.status.success(),
                        "{backend}/{optimization}/{mode}: {output:?}"
                    );
                }
            }
        }
    }
    let _ = lir;
    fs::remove_dir_all(root).unwrap();
}

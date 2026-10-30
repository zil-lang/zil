mod support210;
use support210::*;
use zil_llvm_api::{model::*, validate_module};

fn linear() -> Module {
    m(vec![Item::Function(f(
        "main",
        vec![],
        Ty::Int(32),
        vec![
            b(
                "entry",
                vec![
                    i(
                        "slot",
                        Op::Alloca {
                            ty: Ty::Int(32),
                            align: 4,
                        },
                    ),
                    i("", Op::Branch("first".into())),
                ],
            ),
            b(
                "first",
                vec![
                    // Both stores have side effects even though they have no
                    // SSA result; any schedule must preserve their order.
                    i(
                        "",
                        Op::Store {
                            arg: a(Ty::Int(32), 10),
                            ptr: val("%slot"),
                            align: 4,
                        },
                    ),
                    i(
                        "",
                        Op::Store {
                            arg: a(Ty::Int(32), 20),
                            ptr: val("%slot"),
                            align: 4,
                        },
                    ),
                    i(
                        "seed",
                        Op::Load {
                            ty: Ty::Int(32),
                            ptr: val("%slot"),
                            align: 4,
                        },
                    ),
                    i("", Op::Branch("second".into())),
                ],
            ),
            b(
                "second",
                vec![
                    i(
                        "answer",
                        Op::Binary {
                            opcode: "add".into(),
                            ty: Ty::Int(32),
                            left: val("%seed"),
                            right: val(22),
                        },
                    ),
                    i(
                        "",
                        Op::Store {
                            arg: a(Ty::Int(32), "%answer"),
                            ptr: val("%slot"),
                            align: 4,
                        },
                    ),
                    i(
                        "readback",
                        Op::Load {
                            ty: Ty::Int(32),
                            ptr: val("%slot"),
                            align: 4,
                        },
                    ),
                    i("", Op::Branch("exit".into())),
                ],
            ),
            b(
                "exit",
                vec![i("", Op::Return(Some(a(Ty::Int(32), "%readback"))))],
            ),
        ],
    ))])
}

fn blocks(module: &mut Module) -> &mut Vec<Block> {
    let Item::Function(function) = &mut module.items[0] else {
        unreachable!()
    };
    function.blocks.as_mut().unwrap()
}

fn variants() -> Vec<Module> {
    let mut original = linear();
    let body = blocks(&mut original).clone();
    let mut variants = Vec::new();
    // Every permutation of the three non-entry blocks: natural dependency
    // order, and five valid forward-reference orders requiring the fallback.
    for order in [
        [1, 2, 3],
        [1, 3, 2],
        [2, 1, 3],
        [2, 3, 1],
        [3, 1, 2],
        [3, 2, 1],
    ] {
        let mut module = original.clone();
        *blocks(&mut module) = std::iter::once(body[0].clone())
            .chain(order.map(|index| body[index].clone()))
            .collect();
        variants.push(module);
    }
    let mut natural_loop = loop_module();
    blocks(&mut natural_loop).swap(1, 2);
    variants.extend([natural_loop, loop_module()]);
    variants
}

#[test]
fn textual_block_permutations_and_phi_backedges_are_all_valid() {
    for module in variants() {
        validate_module(&module).unwrap();
    }
}

#[test]
fn unreachable_non_phi_cycles_still_reach_the_rejecting_fallback() {
    let mut module = simple();
    for (name, result, dependency) in [("dead.a", "a", "b"), ("dead.b", "b", "a")] {
        blocks(&mut module).push(b(
            name,
            vec![
                i(result, Op::Freeze(a(Ty::Int(32), local(dependency.into())))),
                i("", Op::Return(Some(a(Ty::Int(32), local(result.into()))))),
            ],
        ));
    }
    assert!(
        validate_module(&module)
            .unwrap_err()
            .message
            .contains("cycle in non-phi")
    );
    blocks(&mut module).swap(1, 2);
    assert!(
        validate_module(&module)
            .unwrap_err()
            .message
            .contains("cycle in non-phi")
    );
    blocks(&mut module)[1].instructions[0].op = Op::Freeze(a(Ty::Int(32), 42));
    validate_module(&module).unwrap();
}

#[test]
fn natural_order_does_not_skip_phi_or_dominance_checks() {
    let mut module = loop_module();
    blocks(&mut module).swap(1, 2);
    let Op::Phi { incoming, .. } = &mut blocks(&mut module)[1].instructions[0].op else {
        unreachable!()
    };
    incoming.pop();
    assert!(
        validate_module(&module)
            .unwrap_err()
            .message
            .contains("predecessor multiset")
    );

    let mut module = linear();
    blocks(&mut module)[1].instructions[2].op = Op::Freeze(a(Ty::Int(32), "%answer"));
    assert!(
        validate_module(&module)
            .unwrap_err()
            .message
            .contains("does not dominate")
    );
}

#[cfg(feature = "native")]
#[test]
fn natural_and_fallback_schedules_preserve_side_effects_and_loop_results() {
    use std::{fs, process::Command};
    use zil_llvm_api::{Config, Optimization, compile_module};
    let path = std::env::temp_dir().join(format!("zil-cp06-schedule-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    for module in variants() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let object = compile_module(
                &module,
                &Config {
                    triple: "x86_64-unknown-linux-gnu",
                    optimization,
                },
                &|| false,
            )
            .unwrap();
            assert!(object.receipt.verify_before_after);
            fs::write(path.join("unit.o"), object.bytes).unwrap();
            let link =
                Command::new(std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into()))
                    .arg(path.join("unit.o"))
                    .arg("-o")
                    .arg(path.join("program"))
                    .output()
                    .unwrap();
            assert!(
                link.status.success(),
                "{}",
                String::from_utf8_lossy(&link.stderr)
            );
            let result = Command::new(path.join("program")).output().unwrap();
            assert_eq!(result.status.code(), Some(42));
            assert!(result.stdout.is_empty() && result.stderr.is_empty());
        }
    }
}

//! Source-derived untrusted Core member metadata. Actual CT binding is covered
//! separately by the frozen project source; this fixture isolates Types paths.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;
fn expression(e: &mut CoreExpr, member: &str, owner: &Type) -> usize {
    let mut count = 0;
    match &mut e.kind {
        CoreExprKind::Member {
            object,
            member: name,
            owner: claim,
        } => {
            count += expression(object, member, owner);
            if name == member {
                *claim = Some(owner.clone());
                count += 1;
            }
        }
        CoreExprKind::Ascribed { value, .. }
        | CoreExprKind::IntegerConvert { value, .. }
        | CoreExprKind::Unary { operand: value, .. }
        | CoreExprKind::Try(value)
        | CoreExprKind::CompileTime(value) => count += expression(value, member, owner),
        CoreExprKind::Binary { left, right, .. }
        | CoreExprKind::Index {
            object: left,
            index: right,
        } => {
            count += expression(left, member, owner);
            count += expression(right, member, owner);
        }
        CoreExprKind::Call { callee, arguments } => {
            count += expression(callee, member, owner);
            for arg in arguments {
                count += expression(arg, member, owner);
            }
        }
        CoreExprKind::TypeApply { callee, .. } => count += expression(callee, member, owner),
        CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
            for field in fields {
                count += expression(field, member, owner);
            }
        }
        CoreExprKind::Record { fields, .. } => {
            for (_, field) in fields {
                count += expression(field, member, owner);
            }
        }
        CoreExprKind::RecordUpdate { base, fields, .. } => {
            count += expression(base, member, owner);
            for (_, field) in fields {
                count += expression(field, member, owner);
            }
        }
        CoreExprKind::Closure { body, .. } => count += block(body, member, owner),
        CoreExprKind::Sequence { bindings, value } => {
            for (_, field) in bindings {
                count += expression(field, member, owner);
            }
            count += expression(value, member, owner);
        }
        CoreExprKind::Slice { object, start, end } => {
            count += expression(object, member, owner);
            for e in [start, end].into_iter().flatten() {
                count += expression(e, member, owner);
            }
        }
        CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            count += expression(condition, member, owner);
            count += expression(then_value, member, owner);
            count += expression(else_value, member, owner);
        }
        _ => {}
    }
    count
}
fn block(b: &mut CoreBlock, member: &str, owner: &Type) -> usize {
    let mut n = 0;
    for statement in &mut b.statements {
        match &mut statement.kind {
            CoreStatementKind::Binding { initializer: Some(e), .. } => n += expression(e, member, owner),
            CoreStatementKind::Assign { target, value } => {
                n += expression(target, member, owner);
                n += expression(value, member, owner);
            }
            CoreStatementKind::Expression(e) => n += expression(e, member, owner),
            CoreStatementKind::Return(Some(e)) => n += expression(e, member, owner),
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                n += expression(condition, member, owner);
                n += block(then_body, member, owner);
                if let Some(b) = else_body {
                    n += block(b, member, owner);
                }
            }
            CoreStatementKind::While { condition, body } => {
                n += expression(condition, member, owner);
                n += block(body, member, owner);
            }
            CoreStatementKind::ForRange {
                start, end, body, ..
            } => {
                n += expression(start, member, owner);
                n += expression(end, member, owner);
                n += block(body, member, owner);
            }
            CoreStatementKind::Block(b)
            | CoreStatementKind::Unsafe(b)
            | CoreStatementKind::Defer(b) => n += block(b, member, owner),
            CoreStatementKind::Match { value, arms } => {
                n += expression(value, member, owner);
                for arm in arms {
                    n += block(&mut arm.body, member, owner);
                }
            }
            CoreStatementKind::IfLet {
                value,
                then_body,
                else_body,
                ..
            } => {
                n += expression(value, member, owner);
                n += block(then_body, member, owner);
                if let Some(b) = else_body {
                    n += block(b, member, owner);
                }
            }
            CoreStatementKind::WhileLet { value, body, .. } => {
                n += expression(value, member, owner);
                n += block(body, member, owner);
            }
            _ => {}
        }
    }
    n
}
fn core(source: &str, owner: &str) -> (CoreModule, Type) {
    let text =
        format!("struct Other{{pub x:i32}}\n{source}\nfn __owner_anchor(value:{owner}){{}}\n");
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "owner.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let module = zil_expand::expand(&unit, &syntax).unwrap();
    let expected = module
        .functions
        .iter()
        .find(|f| f.name == "__owner_anchor")
        .unwrap()
        .parameters[0]
        .ty
        .clone();
    (module, expected)
}
#[test]
fn valid_owner_preserves_complete_typed_ir_and_wrong_nominal_is_rejected() {
    let cases = [
        (
            "struct Point{pub x:i32} fn read(p:Point)->i32{return p.x}",
            "Point",
            "x",
        ),
        (
            "struct Point{pub x:i32} fn read(p:&Point)->i32{return p.x}",
            "Point",
            "x",
        ),
        (
            "struct Point{pub x:i32} fn write(p:&mut Point)->i32{p.x=4;return p.x}",
            "Point",
            "x",
        ),
        (
            "struct Point{pub x:i32} fn read(p:&Point)->i32{let view=&p.x;return *view}",
            "Point",
            "x",
        ),
        (
            "resource R{n:i32} package Owner{manages r:R;pub x:i32} fn read(p:Owner)->i32{let first=p.x;return first+p.x}",
            "Owner",
            "x",
        ),
        ("resource R{x:i32} fn read(r:R)->i32{return r.x}", "R", "x"),
        ("resource R{x:i32} fn read(r:&R)->i32{return r.x}", "R", "x"),
        (
            "struct Point{pub x:i32} struct Outer{pub inner:Point} fn read(p:Outer)->i32{return p.inner.x}",
            "Outer",
            "inner",
        ),
        (
            "struct Point{pub x:i32} struct Outer{pub inner:Point} fn read(p:Outer)->i32{return p.inner.x}",
            "Point",
            "x",
        ),
        (
            "resource R{x:i32} fn read()->i32{let r=R{x:1};r.x=2;return r.x}",
            "R",
            "x",
        ),
        (
            "struct Point{pub x:i32} fn make(n:&mut i32)->Point{*n+=1;return Point{x:3}} fn read()->i32{var n=0;let x=make(&mut n).x;return x+n}",
            "Point",
            "x",
        ),
        (
            "resource R{n:i32} package Owner{manages r:R;pub x:i32} package Outer{pub inner:Owner} fn read()->i32{let p=Outer{inner:Owner{r:R{n:2},x:3}};let f=move()=>p.inner.x+p.inner.x;return f()}",
            "Owner",
            "x",
        ),
        (
            "struct Point{pub x:i32} impl Point{fn read(self:&Self)->i32{return self.x}} fn invoke(p:Point)->i32{return p.read()}",
            "Point",
            "read",
        ),
        (
            "resource R{n:i32} package Owner{manages r:R;pub x:i32} fn read()->i32{let p=Owner{r:R{n:2},x:3};let f=move()=>p.x+p.x;return f()}",
            "Owner",
            "x",
        ),
    ];
    for (case_index, (source, expected, member)) in cases.into_iter().enumerate() {
        let (ordinary, owner) = core(source, expected);
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let baseline = zil_types::check(&ordinary, &target, &limits)
            .unwrap_or_else(|e| panic!("baseline {source}: {e:?}"));
        let mut good = ordinary.clone();
        let mut n = 0;
        for function in &mut good.functions {
            n += block(&mut function.body, member, &owner);
        }
        assert!(n > 0);
        let typed = zil_types::check(&good, &target, &limits)
            .unwrap_or_else(|e| panic!("good {source}: {e:?}"));
        assert_eq!(typed, baseline, "extra/missing operation for {source}");
        let mut bad = ordinary;
        for function in &mut bad.functions {
            block(&mut function.body, member, &Type::Named("Other".into()));
        }
        let errors = zil_types::check(&bad, &target, &limits).unwrap_err();
        if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
            let dir = std::path::PathBuf::from(root).join(format!("member-owner-{case_index}"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("source.txt"), source).unwrap();
            std::fs::write(dir.join("constrained-core.txt"), format!("{good:#?}")).unwrap();
            std::fs::write(dir.join("typed-identical.txt"), format!("{typed:#?}")).unwrap();
            std::fs::write(
                dir.join("wrong-owner-diagnostics.txt"),
                format!("{errors:#?}"),
            )
            .unwrap();
        }

        assert!(
            errors.iter().any(|e| e.code == "E-MEMBER-OWNER"),
            "wrong owner {source}: {errors:?}"
        );
    }
}
#[test]
fn phantom_generic_arguments_are_part_of_member_owner_identity() {
    let source = "struct Phantom<T>{pub x:i32} fn read(p:Phantom<i32>)->i32{return p.x}";
    let (mut good, owner) = core(source, "Phantom<i32>");
    for f in &mut good.functions {
        block(&mut f.body, "x", &owner);
    }
    zil_types::check(&good, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let (mut bad, owner) = core(source, "Phantom<i64>");
    for f in &mut bad.functions {
        block(&mut f.body, "x", &owner);
    }
    let errors =
        zil_types::check(&bad, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == "E-MEMBER-OWNER"),
        "{errors:?}"
    );
}

#[test]
fn member_owner_work_has_an_exact_request_limit_and_fresh_replay() {
    let (mut module, owner) = core(
        "struct Point{pub x:i32} fn read(p:Point)->i32{return p.x}",
        "Point",
    );
    for function in &mut module.functions {
        block(&mut function.body, "x", &owner);
    }
    let target = TargetProfile::host_llvm();
    let defaults = Limits::default();
    let expected = zil_types::check(&module, &target, &defaults).unwrap();
    let (mut low, mut high) = (0, defaults.max_analysis_steps);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: mid,
            ..defaults.clone()
        };
        match zil_types::check(&module, &target, &limits) {
            Ok(actual) => {
                assert_eq!(actual, expected);
                high = mid;
            }
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|e| e.category == zil_base::ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                low = mid;
            }
        }
    }
    let exact = Limits {
        max_analysis_steps: high,
        ..defaults.clone()
    };
    assert_eq!(
        zil_types::check(&module, &target, &exact).unwrap(),
        expected
    );
    let below = Limits {
        max_analysis_steps: high - 1,
        ..defaults
    };
    let errors = zil_types::check(&module, &target, &below).unwrap_err();
    assert!(
        errors
            .iter()
            .all(|e| e.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert_eq!(
        zil_types::check(&module, &target, &exact).unwrap(),
        expected
    );
    eprintln!("Types member owner N={high}; N-1 ResourceLimit; fresh N identical TypedModule");
}

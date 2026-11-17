use super::*;
use zil_base::{FileId, SourceUnit};

fn module(text: &str) -> Module {
    let source = SourceUnit::new(FileId(0), zil_base::ModuleId(0), "plain.zil", text);
    zil_syntax::parse(&source).into_result().unwrap()
}

fn observed(item: &Item, limits: &Limits) -> Option<crate::lifetimes::scalar::Demand> {
    let mut probe = Probe {
        budget: Budget::new(limits),
        present: false,
        tasks: false,
        scalar: crate::lifetimes::scalar::Probe::new(item),
        view: None,
    };
    crate::ct_selection::Visitor::item(&mut probe, item, 0).unwrap();
    probe
        .scalar
        .take()?
        .finish(limits, &probe.budget.remaining_limits())
}

#[test]
fn current_probe_lifetime_demand_matches_real_walk_at_all_quota_prefixes() {
    let input = module(
        "extern \"C\" fn foreign(x:i32)->i32;\n\
         #[test] fn f(x:(i32),y:bool)->(i32){\n\
           let z:i32=dep.call(x+1);\n\
           if y {let x=z;while x>0 {if x==2 {break;}else{continue;}}}\n\
           else if x<0 {unsafe{foreign(x);}}else{loop{break;}}\n\
           return if y {z} else {x};\n\
         }\n\
         fn unused()->i32=>f(1,true); fn empty(){}",
    );
    let limits = Limits::default();
    for item in &input.items {
        let demand = observed(item, &limits).expect("complete scalar input");
        let mut actual = Budget::new(&limits);
        crate::lifetimes::validate_items_cached(std::iter::once(item), &mut actual, None).unwrap();
        let remaining = actual.remaining_limits();
        let costs = [
            limits.max_nodes - remaining.max_nodes,
            limits.max_analysis_steps - remaining.max_analysis_steps,
        ];
        for (kind, cost) in costs.into_iter().enumerate() {
            for amount in 0..=cost + 1 {
                let mut allowance = limits.clone();
                if kind == 0 {
                    allowance.max_nodes = amount;
                } else {
                    allowance.max_analysis_steps = amount;
                }
                let mut shared = Budget::new(&allowance);
                let fits = demand.debit(item, &mut shared).is_some();
                let mut ordinary = Budget::new(&allowance);
                let result = crate::lifetimes::validate_items_cached(
                    std::iter::once(item),
                    &mut ordinary,
                    None,
                );
                assert_eq!(
                    fits,
                    result.is_ok(),
                    "kind={kind} amount={amount} span={:?}",
                    item.span
                );
                if fits {
                    assert_eq!(shared.remaining_limits(), ordinary.remaining_limits());
                } else {
                    assert_eq!(
                        shared.remaining_limits(),
                        allowance,
                        "failed debit is atomic"
                    );
                    let fallback = crate::lifetimes::validate_items_cached(
                        std::iter::once(item),
                        &mut shared,
                        None,
                    );
                    assert_eq!(fallback, result, "retain exact error and span");
                }
            }
        }
        for depth in 0..=limits.max_call_depth {
            let allowance = Limits {
                max_call_depth: depth,
                ..limits.clone()
            };
            let mut shared = Budget::new(&allowance);
            let mut ordinary = Budget::new(&allowance);
            assert_eq!(
                demand.debit(item, &mut shared).is_some(),
                crate::lifetimes::validate_items_cached(std::iter::once(item), &mut ordinary, None)
                    .is_ok(),
                "depth={depth} span={:?}",
                item.span,
            );
        }
    }
}

#[test]
fn lifetime_demand_rejects_a_different_item_with_the_same_span() {
    let input = module("fn f(x:i32)->i32=>x+1;");
    let limits = Limits::default();
    let item = &input.items[0];
    let demand = observed(item, &limits).unwrap();
    let mut replacement = item.clone();
    let ItemKind::Function(function) = &mut replacement.kind else {
        unreachable!()
    };
    function.name = "different".into();
    assert_eq!(replacement.span, item.span);
    let mut budget = Budget::new(&limits);
    assert!(demand.debit(&replacement, &mut budget).is_none());
    assert_eq!(budget.remaining_limits(), limits);
    assert!(demand.debit(item, &mut budget).is_some());
}

#[test]
fn current_probe_refuses_rich_types_and_unselected_body_features() {
    let limits = Limits::default();
    for source in [
        "fn f<life a>(x:&'a i32)->i32=>1;",
        "fn f(x:&'missing i32)->i32=>1;",
        "fn f(x:SomeNominal)->i32=>1;",
        "fn f()->i32{if true{return 1;}else{return task_create(worker);}}",
        "fn f()->i32{if true{return 1;}else{defer{} return 2;}}",
        "fn f()->i32{let (a,b)=(1,2);return a;}",
    ] {
        let input = module(source);
        assert!(observed(&input.items[0], &limits).is_none(), "{source}");
    }
}

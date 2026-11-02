//! Standalone source probe linked to explicitly selected read-only baseline or
//! candidate rlibs. This is front-end cost evidence, not native program evidence.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
const CASES: &[(&str, &str)] = &[
    (
        "copy_scalar_helper",
        "fn apply<T:Copy,F:Fn(i32)->i32>(f:&F,value:T)->T{f(1);return value}fn main()->i32{let f=(n:i32)=>n+1;return apply(&f,2)-2}",
    ),
    ("copy_enum_helper", include_str!("copy_enum_helper.zil")),
    (
        "old_flat_product",
        "resource R{n:i32}package P{pad:i32;manages old:R;wide:[i32;2];manages fresh:R}fn origin(give old:R)->R contract{result(create)}{let p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let q=p;return give q.fresh}fn main(){}",
    ),
];
fn minimum(ok: impl Fn(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, Limits::default().max_analysis_steps);
    assert!(ok(hi));
    while lo + 1 < hi {
        let n = lo + (hi - lo) / 2;
        if ok(n) { hi = n } else { lo = n }
    }
    hi
}
fn main() {
    for (name, source) in CASES {
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget246.zil", *source);
        let ast = zil_syntax::parse(&unit).into_result().unwrap();
        let expand = |n| {
            zil_expand::expand_with_map_limits(
                &unit,
                &ast,
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
        };
        let core = expand(Limits::default().max_analysis_steps).unwrap().core;
        let types = |n| {
            zil_types::check(
                &core,
                &TargetProfile::host_llvm(),
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
        };
        let expected = types(Limits::default().max_analysis_steps).unwrap();
        let e = minimum(|n| expand(n).is_ok());
        let t = minimum(|n| types(n).is_ok());
        assert!(
            expand(e - 1)
                .unwrap_err()
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit)
        );
        assert!(
            types(t - 1)
                .unwrap_err()
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(expand(e).unwrap().core, core);
        assert_eq!(expand(e).unwrap().core, core);
        assert_eq!(types(t).unwrap(), expected);
        assert_eq!(types(t).unwrap(), expected);
        println!(
            "{{\"case\":\"{name}\",\"expand\":{e},\"types\":{t},\"n_minus_1\":\"ResourceLimit\",\"fresh_n\":\"identical\"}}"
        );
    }
}

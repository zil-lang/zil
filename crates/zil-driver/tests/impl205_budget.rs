//! Exact author cost boundaries. Historical runs preserve their own production.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};

const OLD_STATE: &str = include_str!("impl197_cases/01-existing-state-f-sibling/main.zil");
const EAGER: &str =
    "fn identity<T:Copy>(value:T)->T{return value}fn main()->i32{return identity(42)-42}";
const NEW: &str = "package State<F>{manages bytes:ByteBuffer;a:F;b:F}fn make(n:i32)->impl Fn()->i32{return move()=>n}fn pack<F:Fn()->i32>(a:F,b:F)->State<F>{return State::<F>{bytes:bytes_new(),a:a,b:b}}fn replace<F:Fn()->i32>(state:State<F>,replacement:F)->State<F>{var next=state;next.a=replacement;return next}fn main()->i32{let state=pack(make(1),make(2));let changed=replace(state,make(3));return changed.a()+changed.b()-5}";

fn boundary(label: &str, source: &str) -> usize {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("budget205-{label}-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("source.zil"), source).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget205.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    fs::write(root.join("core.txt"), format!("{core:#?}")).unwrap();
    let check = |n| {
        zil_types::check(
            &core,
            &TargetProfile::host_llvm(),
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    check(high).unwrap_or_else(|e| panic!("{label}: {e:#?}"));
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        match check(mid) {
            Ok(_) => high = mid,
            Err(e) => {
                assert!(
                    e.iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit),
                    "{label}: {e:#?}"
                );
                low = mid;
            }
        }
    }
    // 205 closes the public unused-legacy-State gap. The exact same Core
    // required 2642 steps before that definition walk; record its actual
    // ResourceLimit now instead of silently preserving an undercharged path.
    if label == "old-state" {
        let prior = check(2642).unwrap_err();
        assert!(
            prior
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit)
        );
        fs::write(root.join("prior-N-2642.txt"), format!("{prior:#?}")).unwrap();
    }
    let exact = check(high).unwrap();
    let below = check(high - 1).unwrap_err();
    assert!(
        below
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    let fresh = check(high).unwrap();
    assert_eq!(exact, fresh);
    fs::write(root.join("N-1.txt"), format!("{below:#?}")).unwrap();
    fs::write(root.join("N.txt"), format!("{exact:#?}")).unwrap();
    fs::write(root.join("boundary.json"),format!("{{\"types_work\":{high},\"checks\":\"N / N-1 ResourceLimit / fresh N exact equality\"}}\n")).unwrap();
    println!("{label}: Types N={high}");
    high
}

#[test]
fn actual_types_boundaries_cover_new_state_and_legacy_controls() {
    for (label, source) in [
        ("old-state", OLD_STATE),
        ("ordinary-eager", EAGER),
        ("replacement", NEW),
    ] {
        let expected = match label {
            "old-state" => 5293,
            "ordinary-eager" => 88,
            "replacement" => 4498,
            _ => unreachable!(),
        };
        assert_eq!(boundary(label, source), expected, "{label}");
    }
}

#[test]
fn actual_types_boundary_covers_refreshed_alias_and_continue_snapshots() {
    let source = "package State<F>{manages bytes:ByteBuffer;a:F;b:F}fn recycle<F:Fn()->i32>(state:State<F>)->State<F>{var next=state;var view=&next.a;while view()==0 {let moved=next.a;next.a=moved;view=&next.a;continue}view();return next}fn main(){}";
    boundary("loop-refresh-continue", source);
}

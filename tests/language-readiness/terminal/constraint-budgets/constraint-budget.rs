//! Standalone main-thread baseline/candidate work-boundary comparison.
fn main() {
    let path = std::env::args().nth(1).expect("source path");
    let text = std::fs::read_to_string(&path).unwrap();
    let source = zil_base::SourceUnit::new(zil_base::FileId(0), zil_base::ModuleId(0), path, text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let defaults = zil_base::Limits::default();
    let succeeds = |steps| {
        zil_types::infer_source_closures(
            &core,
            &zil_base::Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            },
        )
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    assert!(succeeds(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if succeeds(middle).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(succeeds(low).is_ok());
    assert!(succeeds(low).is_ok());
    assert_eq!(
        succeeds(low - 1).unwrap_err()[0].category,
        zil_base::ResultCategory::ResourceLimit
    );
    println!("source constraint N={low}");
}

//! Measure an existing request boundary without changing its default limits.
//! Callers retain fixed recorded minima and N/N-1/fresh semantic assertions.
pub fn minimum<T>(
    work: bool,
    mut run: impl FnMut(&zil_base::Limits) -> zil_base::CompileResult<T>,
) -> usize {
    let defaults = zil_base::Limits::default();
    assert!(run(&defaults).is_ok(), "default request must succeed");
    let (mut low, mut high) = (
        0,
        if work {
            defaults.max_analysis_steps
        } else {
            defaults.max_nodes
        },
    );
    while high - low > 1 {
        let mid = low + (high - low) / 2;
        let mut limits = defaults.clone();
        if work {
            limits.max_analysis_steps = mid;
        } else {
            limits.max_nodes = mid;
        }
        match run(&limits) {
            Ok(_) => high = mid,
            Err(errors) => {
                assert!(
                    !errors.is_empty()
                        && errors
                            .iter()
                            .all(|e| e.category == zil_base::ResultCategory::ResourceLimit),
                    "boundary probe must only fail its request limit: {errors:?}"
                );
                low = mid;
            }
        }
    }
    high
}

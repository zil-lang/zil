//! Explicit SDK consumer calibration; ordinary test and library defaults stay standard.
pub fn limits() -> zil_base::Limits {
    let profile = std::env::var("ZIL_TEST_WORK_PROFILE").unwrap_or_else(|_| "standard".into());
    let mut limits = zil_base::Limits::default();
    match profile.as_str() {
        "standard" => {}
        "alpha" => limits.max_analysis_steps = 25_000_000,
        _ => panic!("unknown explicit ZIL_TEST_WORK_PROFILE: {profile}"),
    }
    eprintln!("SDK consumer work_profile={profile} effective_limits={limits:?}");
    limits
}

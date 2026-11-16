//! Explicit SDK consumer calibration; ordinary test and library defaults stay standard.
pub fn limits() -> zil_base::Limits {
    let profile = std::env::var("ZIL_TEST_WORK_PROFILE").unwrap_or_else(|_| "standard".into());
    let mut limits = zil_base::Limits::default();
    let revision = match profile.as_str() {
        "standard" => "standard-10m-v1",
        "alpha" => {
            limits.max_analysis_steps = 64_000_000;
            "alpha-64m-v2"
        }
        _ => panic!("unknown explicit ZIL_TEST_WORK_PROFILE: {profile}"),
    };
    eprintln!(
        "SDK consumer work_profile={profile} work_profile_revision={revision} effective_limits={limits:?}"
    );
    limits
}

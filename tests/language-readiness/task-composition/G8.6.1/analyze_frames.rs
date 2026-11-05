//! Actual-source resource-entry attacks for the new producer frame profile.
use super::*;

#[test]
fn g861_analyze_enum_leaf_teardown_entry_binds_actual_resource() {
    let text = format!(
        "{OWNED_ENUM861}\nresource Other{{code:i32,extra:i64}}impl Drop for Other{{fn drop(self:Self){{unsafe{{putchar(.code);}}}}}}"
    );
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let typed = zil_types::check(&expand(&text, &limits).unwrap().core, &target, &limits).unwrap();
    zil_analyze::analyze(&typed, &limits).unwrap();
    let token = typed
        .resources
        .iter()
        .position(|r| r.name.ends_with("Token"))
        .unwrap();
    let other = typed
        .resources
        .iter()
        .find(|r| r.name.ends_with("Other"))
        .unwrap();
    let foreign = typed
        .functions
        .iter()
        .find(|f| f.foreign.is_some())
        .unwrap()
        .id;
    for (case, teardown) in [
        ("wrong-resource", other.teardown.unwrap()),
        ("foreign-helper", foreign),
    ] {
        let mut raw = typed.clone();
        raw.resources[token].teardown = Some(teardown);
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze accepted {case}"
        );
    }
    for attack in 0..3 {
        let mut raw = typed.clone();
        match attack {
            0 => raw.resources[token].name.push_str(".forged"),
            1 => raw.resources.push(raw.resources[token].clone()),
            _ => {
                let other = raw
                    .resources
                    .iter()
                    .position(|r| r.name.ends_with("Other"))
                    .unwrap();
                raw.resources[other].name = raw.resources[token].name.clone();
            }
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze accepted resource identity attack {attack}"
        );
    }
}

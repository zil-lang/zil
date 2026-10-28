//! Store admission must retain the pre-existing concrete-resource helper mode.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn check(text: &str, accepted: bool) {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "helper-give178-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("input.zil"), text).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "helper178.zil", text),
        &CompileOptions::default(),
    );
    fs::write(path.join("compile.json"), report.to_json()).unwrap();
    if accepted {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1)
    } else {
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand);
        assert_eq!(report.verification_runs, 0)
    }
}
#[test]
fn concrete_byte_buffer_uses_existing_helper_owned_parameter_contract() {
    check(
        "fn accept<F:Fn()->i64>(f:&F,give bytes:ByteBuffer)->i64{return f()+bytes_len(bytes)}fn main()->i32{let f=()->i64=>2;let bytes=bytes_new();return i32(accept(&f,give bytes))-2}",
        true,
    );
}
#[test]
fn give_does_not_turn_unknown_or_copy_values_into_resources() {
    for source in [
        "fn bad<T>(give item:T){}fn main(){}",
        "fn bad<T:StoreElement>(give item:T){}fn main(){}",
        "trait Marker{}fn bad<T:Marker>(give item:T){}fn main(){}",
        "fn bad<F:Fn()->i32>(f:&F,give item:i32)->i32=>f();fn main(){}",
        "resource R{n:i32}package P{manages r:R}fn bad<F:Fn()->i32>(f:&F,give item:P)->i32=>f();fn main(){}",
        "fn bad<F:Fn()->i32>(f:&F,give item:&i32)->i32=>f();fn main(){}",
    ] {
        check(source, false)
    }
}

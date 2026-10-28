#![cfg(not(feature = "native"))]
#[test]
fn unavailable_is_explicit_and_does_not_require_llvm() {
    assert!(!zil_llvm_api::available());
    let e = zil_llvm_api::compile(
        "",
        &zil_llvm_api::Config {
            triple: "x86_64-unknown-linux-gnu",
            optimization: zil_llvm_api::Optimization::O0,
        },
        &|| false,
    )
    .unwrap_err();
    assert_eq!(e.kind, zil_llvm_api::ErrorKind::Unavailable);
}

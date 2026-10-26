import conformance.support.closure_runtime as fx
fn invoke_shared<F: Fn() -> void>(callback: F) {
    callback()
}
fn check() {
    let file = fx.make(17)
    let close = move () => fx.close(file)
    invoke_shared(close) // expected-error: E-CLOSURE-CALL-KIND
}

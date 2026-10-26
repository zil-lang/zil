import conformance.support.closure_runtime as fx
fn check() {
    let file = fx.make(17)
    let alias = file
    let close = () => fx.close(alias)
    close()
    close() // expected-error: E-CALL-PROTOCOL-STATE
}

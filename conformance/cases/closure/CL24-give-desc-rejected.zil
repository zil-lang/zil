import conformance.support.closure_runtime as fx
fn check() -> i64 {
    let file = fx.make(17)
    let alias = file
    let consume = () => fx.take(give alias) // expected-error: E-GIVE-DESCRIPTOR
    return consume()
}

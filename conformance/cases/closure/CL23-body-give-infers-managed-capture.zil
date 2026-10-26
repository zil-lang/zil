import conformance.support.closure_runtime as fx
fn make_consumer() -> impl FnOnce() -> i64 {
    let file = fx.make(17)
    return () => fx.take(give file)
}
fn check() -> i64 {
    let consume = make_consumer()
    return consume()
}

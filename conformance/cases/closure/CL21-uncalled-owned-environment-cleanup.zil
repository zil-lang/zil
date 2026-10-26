import conformance.support.closure_runtime as fx
fn check() -> i64 {
    let file = fx.make(17)
    let unused = move () => fx.read(file)
    return 0
}

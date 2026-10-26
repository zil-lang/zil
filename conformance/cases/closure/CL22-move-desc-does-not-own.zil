import conformance.support.closure_runtime as fx
fn make_reader() -> impl Fn() -> i64 {
    let file = fx.make(17)
    let alias = file
    return move () => fx.read(alias) // expected-error: E-CAPTURE-ESCAPE
}

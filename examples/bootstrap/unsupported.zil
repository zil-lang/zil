// One address layer is supported; a reference to an address remains unsupported.
fn main() -> i32 {
    var value = 42
    let p = &mut value
    let pp = &p
    return **pp
}

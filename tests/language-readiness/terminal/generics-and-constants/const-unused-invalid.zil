// This unused template is invalid for arbitrary T. A concrete i32 call must
// never supply the missing numeric capability to its definition.
fn invalid<T, #N: i32>(value: T) -> T {
    return value + value
}

fn main() -> i32 {
    return 0
}

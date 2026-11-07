import mutable_holder_api as holder

fn main() -> i32 {
    var value: i64 = 40;
    var window = holder.make(&mut value);
    let first = window.pointer;
    let second = window.pointer;
    *first = *first + 1;
    *second = *second + 1;
    return i32(value);
}

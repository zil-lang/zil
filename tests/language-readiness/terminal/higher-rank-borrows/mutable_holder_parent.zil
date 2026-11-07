import mutable_holder_api as holder

fn main() -> i32 {
    var value: i64 = 40;
    var window = holder.make(&mut value);
    let first = window.pointer;
    holder.increment(&mut window);
    *first = *first + 1;
    return i32(value);
}

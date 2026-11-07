import mutable_holder_api as holder

fn main() -> i32 {
    var value: i64 = 40;
    var window = holder.make(&mut value);
    holder.increment(&mut window);
    holder.increment(&mut window);
    return i32(value);
}

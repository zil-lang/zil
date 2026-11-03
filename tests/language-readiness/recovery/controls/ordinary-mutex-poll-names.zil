struct Mutex { value: i32 }
fn poll(value: &mut Mutex) { value.value += 1 }
fn main() -> i32 {
    var value = Mutex { value: 6 }
    poll(&mut value)
    return value.value - 7
}

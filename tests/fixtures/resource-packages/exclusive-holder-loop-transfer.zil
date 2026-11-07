struct Holder<life a> { pointer: &'a mut i32 }
fn make<life a>(pointer: &'a mut i32) -> Holder<'a> { return Holder::<'a> { pointer: pointer } }

fn keep_two<life a>(first: Holder<'a>, second: Holder<'a>, pointer: &'a mut i32) -> i32 {
    var left = first
    var right = second
    var count = 0
    while count < 2 {
        var next = left
        if count == 0 { left = next } else { left = next }
        count += 1
    }
    *left.pointer = 3
    *right.pointer = 4
    return 0
}

fn main() -> i32 {
    var one = 1
    var two = 2
    var shared = 0
    var left = make(&mut one)
    var right = make(&mut two)
    keep_two(left, right, &mut shared)
    if one == 3 && two == 4 && shared == 0 { return 0 }
    return 1
}

struct Holder<life a> { pointer: &'a mut i32 }
fn make<life a>(pointer: &'a mut i32) -> Holder<'a> { return Holder::<'a> { pointer: pointer } }

fn keep_two<life a>(first: Holder<'a>, second: Holder<'a>, pointer: &'a mut i32) -> i32 {
    var left = first
    var right = second
    var count = 0
    while count < 2 {
        var next = Holder::<'a> { pointer: pointer }
        if count == 0 { left = next } else { right = next }
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
    return keep_two(left, right, &mut shared)
}

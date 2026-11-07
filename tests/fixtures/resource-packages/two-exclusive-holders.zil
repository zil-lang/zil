struct Holder<life a> { pointer: &'a mut i32 }

fn peek(pointer: &mut i32) -> i32 { return *pointer }

fn use_two<life a>(first: &'a mut i32, second: &'a mut i32) -> i32 {
    var left = Holder::<'a> { pointer: first }
    var right = Holder::<'a> { pointer: second }
    *left.pointer = 3
    *right.pointer = 4
    return 0
}

fn main() -> i32 {
    var one = 1
    var two = 2
    use_two(&mut one, &mut two)
    if one == 3 && two == 4 { return 0 }
    return 1
}

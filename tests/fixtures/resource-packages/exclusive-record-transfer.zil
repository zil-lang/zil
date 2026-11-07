struct Holder<life a> { pointer: &'a mut i32 }

fn make<life a>(pointer: &'a mut i32) -> Holder<'a> {
    return Holder::<'a> { pointer: pointer }
}

fn main() -> i32 {
    var number = 1
    var holder = make(&mut number)
    var moved = holder
    *moved.pointer = 3
    if number == 3 { return 0 }
    return 1
}

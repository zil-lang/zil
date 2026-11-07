struct Holder<life a> { pointer: &'a mut i32 }

fn make<life a>(pointer: &'a mut i32) -> Holder<'a> {
    return Holder::<'a> { pointer: pointer }
}

fn use_twice<life a>(holder: Holder<'a>) -> i32 {
    var first = holder
    var second = holder
    *first.pointer = 2
    *second.pointer = 3
    return 0
}

fn main() -> i32 {
    var number = 1
    return use_twice(make(&mut number))
}

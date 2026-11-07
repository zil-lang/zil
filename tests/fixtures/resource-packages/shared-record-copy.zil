struct Holder<life a> { pointer: &'a i32 }

fn make<life a>(pointer: &'a i32) -> Holder<'a> {
    return Holder::<'a> { pointer: pointer }
}

fn main() -> i32 {
    var number = 2
    var holder = make(&number)
    var first = holder
    var second = holder
    if *first.pointer + *second.pointer == 4 { return 0 }
    return 1
}

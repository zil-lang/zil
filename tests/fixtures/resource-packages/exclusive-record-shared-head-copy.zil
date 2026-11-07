struct Holder<life a> { pointer: &'a mut i32 }

fn make<life a>(pointer: &'a mut i32) -> Holder<'a> {
    return Holder::<'a> { pointer: pointer }
}

fn clone<life a>(holder: &Holder<'a>) -> Holder<'a> {
    return *holder
}

fn main() -> i32 {
    var number = 1
    var holder = make(&mut number)
    var copy = clone(&holder)
    *holder.pointer = 2
    *copy.pointer = 3
    return 0
}

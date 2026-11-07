struct Holder<life a> { pointer: &'a mut i32 }
fn peek(pointer: &mut i32) -> i32 { return *pointer }

fn work<life a>(pointer: &'a mut i32) {
    peek(pointer)
    peek(pointer)
    var borrowed = Holder::<'a> { pointer: &mut *pointer }
    *borrowed.pointer += 1
    *pointer += 1
    var count = 0
    while count < 2 {
        var transient = Holder::<'a> { pointer: &mut *pointer }
        *transient.pointer += 1
        count += 1
    }
    var moved = Holder::<'a> { pointer: pointer }
    *moved.pointer += 1
}

fn main() -> i32 {
    var number = 0
    work(&mut number)
    if number == 5 { return 0 }
    return 1
}

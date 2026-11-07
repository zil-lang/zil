struct Holder<life a> { pointer: &'a i32 }

fn read_copy<life a>(condition: bool, pointer: &'a i32) -> i32 {
    var current = Holder::<'a> { pointer: pointer }
    if condition {
        let previous = move_out(current.pointer)
    }
    var copied = current
    return *copied.pointer
}

fn main() -> i32 {
    var number = 3
    if read_copy(true, &number) + read_copy(false, &number) == 6 { return 0 }
    return 1
}

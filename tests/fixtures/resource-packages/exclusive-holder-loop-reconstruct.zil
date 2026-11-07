struct Holder<life a> { pointer: &'a mut i32 }

fn reconstruct<life a>(pointer: &'a mut i32) {
    var holder = Holder::<'a> { pointer: pointer }
    var count = 0
    while count < 2 {
        let previous = move_out(holder.pointer)
        *previous += 1
        holder = Holder::<'a> { pointer: previous }
        count += 1
    }
    *holder.pointer += 1
}

fn main() -> i32 {
    var number = 0
    reconstruct(&mut number)
    if number == 3 { return 0 }
    return 1
}

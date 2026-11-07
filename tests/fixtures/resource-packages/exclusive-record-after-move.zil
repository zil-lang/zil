struct Holder<life a> { pointer: &'a mut i32 }
fn make<life a>(pointer: &'a mut i32) -> Holder<'a> { return Holder::<'a> { pointer: pointer } }
fn main() -> i32 {
 var n = 1
 var holder = make(&mut n)
 var moved = holder
 *holder.pointer = 2
 *moved.pointer = 3
 return 0
}

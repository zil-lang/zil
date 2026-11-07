struct Holder<life a> { pointer: &'a mut i32 }
fn make<life a>(pointer: &'a mut i32) -> Holder<'a> { return Holder::<'a> { pointer: pointer } }
fn partial<life a>(condition: bool, holder: Holder<'a>) -> Holder<'a> {
 var current = holder
 if condition {
  var moved = current
  *moved.pointer = 4
 } else {
  var keep = 0
 }
 return current
}
fn main() -> i32 {
 var n = 1
 var h = make(&mut n)
 var result = partial(true, h)
 *result.pointer = 4
 return 0
}

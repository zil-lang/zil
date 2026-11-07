struct Holder<life a> { pointer: &'a mut i32 }
fn make<life a>(pointer: &'a mut i32) -> Holder<'a> { return Holder::<'a> { pointer: pointer } }
fn move_both<life a>(condition: bool, holder: Holder<'a>) -> Holder<'a> {
 var current = holder
 if condition {
  var moved = current
  current = moved
 } else {
  var moved = current
  current = moved
 }
 return current
}
fn main() -> i32 {
 var n = 1
 var h = make(&mut n)
 var result = move_both(true, h)
 *result.pointer = 4
 if n == 4 { return 0 }
 return 1
}

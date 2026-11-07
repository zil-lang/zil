struct Pair<life a> { first: &'a mut i32; second: &'a mut i32 }
fn make<life a>(first: &'a mut i32, second: &'a mut i32) -> Pair<'a> {
 return Pair::<'a> { first: first, second: second }
}
fn main() -> i32 {
 var one = 1
 var two = 2
 var pair = make(&mut one, &mut two)
 *pair.first = 3
 *pair.second = 4
 if one == 3 && two == 4 { return 0 }
 return 1
}

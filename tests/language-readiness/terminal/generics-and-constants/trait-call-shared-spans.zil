trait Left<Tag> { fn left(self: &Self) -> i32; }
trait Right<Tag> { fn right(self: &Self) -> i32; }
struct P {}
impl Left<i32> for P { fn left() -> i32 { return 7 } }
impl Right<i32> for P { fn right() -> i32 { return 11 } }
fn choose<T: Left<i32> + Right<i32>>(p: &T) -> i32 {
    return p.left() + p.right()
}
fn main() -> i32 { let p = P {}; return choose(&p) - 18 }

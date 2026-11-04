trait Echo<R: Copy> { fn echo(self: &Self, value: R) -> R; }
struct Box<T: Copy> { value: T }
impl<T: Copy, R: Copy> Echo<R> for Box<T> {
    fn echo(value: R) -> R { return value }
}
fn send<R: Copy, T: Echo<R>>(p: &T, value: R) -> R {
    return p.echo(value)
}
fn relay<U: Copy, V: Echo<U>>(p: &V, value: U) -> U {
    return send::<U, V>(p, value)
}
fn main() -> i32 {
    let p = Box::<bool> { value: true };
    if relay::<bool, Box<bool>>(&p, true) {
        return relay::<i32, Box<bool>>(&p, 7) - 7
    }
    return 1
}

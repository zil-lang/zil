extern "C" fn putchar(value: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package State<T,F,G> { manages marker: Ticket; first: F; second: G; tag: T }
fn make(label: i32, value: i32) -> impl Fn() -> i32 {
    let ticket = Ticket { code: label }
    return move () => { ticket.code; return value }
}
fn pack<T: Copy, F: Fn() -> i32, G: FnOnce() -> i32>(first: F, second: G, tag: T) -> State<T,F,G> {
    return State::<T,F,G> { marker: Ticket { code: 90 }, first: first, second: second, tag: tag }
}
fn run<T: Copy, F: Fn() -> i32, G: FnOnce() -> i32>(state: State<T,F,G>) -> (i32, T) {
    let tag = state.tag
    let first = state.first()
    let again = state.first()
    let second = state.second()
    return (first + again + second - 40, tag)
}
fn main() -> i32 {
    let result = run(pack(make(65, 10), make(66, 20), 17))
    if result.1 != 17 { return 1 }
    return result.0
}

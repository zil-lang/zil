struct State {
    sum: i64
    history: [i64; 2]
    pair: (i64, i64)
}
fn score(s: State) -> i64 { return s.sum + s.history[0] + s.pair.1 }
pub fn answer() -> i64 {
    var state = State { sum: 0, history: [0, 0], pair: (0, 0) }
    var push = move delta => {
        state.sum += delta
        state.history[0] = state.sum * 2
        state.pair.1 = state.sum * 3
        state
    }
    let a = score(push(2))
    var copy = push
    let b = score(push(3))
    let c = score(copy(8))
    return a * 10000 + b * 100 + c
}
fn main() -> i32 {
    if answer() == 123060 { return 0 }
    return 1
}

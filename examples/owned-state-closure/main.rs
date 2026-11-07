// Behavioral counterpart only; no controlled performance comparison is claimed.
#[derive(Clone, Copy)]
struct State {
    sum: i64,
    history: [i64; 2],
    pair: (i64, i64),
}
fn score(s: State) -> i64 {
    s.sum + s.history[0] + s.pair.1
}
fn answer() -> i64 {
    let mut state = State {
        sum: 0,
        history: [0, 0],
        pair: (0, 0),
    };
    let mut push = move |delta: i64| {
        state.sum += delta;
        state.history[0] = state.sum * 2;
        state.pair.1 = state.sum * 3;
        state
    };
    let a = score(push(2));
    let mut copy = push;
    let b = score(push(3));
    let c = score(copy(8));
    a * 10000 + b * 100 + c
}
fn main() {
    println!("{}", answer());
}

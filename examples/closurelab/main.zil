import scoring.weights

// The closure captures the initial base and calls a real imported function.
pub fn answer() -> i64 {
    var base: i64 = #(100 + 20)
    let score = move value => { weights.apply(value, 2) + base }
    base = 999
    let saved = score
    var total: i64 = 0
    let limit: i64 = 4
    for value in 0..limit {
        total = total + saved(value)
    }
    return total
}

fn main() -> i32 {
    if answer() == 492 { return 0 }
    return 1
}

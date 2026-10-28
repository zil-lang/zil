import helpers

pub fn check() -> i32 {
    let narrow = move (n: i32) => n + 1
    let wide = move (n: i64) => n + 2
    let first = helpers.map(&narrow, i32(2))
    let second = helpers.map(&wide, i64(2))
    let same = move () => 1
    let a = helpers.keep(&same, helpers.A { n: 3 })
    let b = helpers.keep(&same, helpers.B { n: 4 })
    let pair = helpers.duplicate(a)
    return first + i32(second) + pair.0.n + pair.1.n + b.n - 17
}

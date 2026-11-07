struct Table { values: [i64; 4]; total: i64 }
fn build(start: i64) -> Table {
    var table = Table { values: [0, 0, 0, 0], total: 0 }
    let end: i64 = 4
    for i in 0..end {
        table.values[i] = start + i
        table.total = table.total + table.values[i]
    }
    return table
}
pub fn result() -> i64 {
    var table = #(build(7))
    let pair: (i64, bool) = #((42, true))
    let values: [i64; 3] = #([1, 2, 3])
    table.values[0] = 50
    if pair.1 { return table.total * 10000 + table.values[0] * 100 + pair.0 + values[2] }
    return 0
}
fn main() -> i32 { if result() == 345045 { return 0 } return 1 }

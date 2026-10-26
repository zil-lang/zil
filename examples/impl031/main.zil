struct Inventory { counts: [i64; 4]; enabled: bool; unit: (); }
fn sum_to(n: i64) -> i64 {
    var sum: i64 = 0
    for i in 0..n { sum = sum + i + 1 }
    return sum
}
fn choose(a: &Inventory, b: &Inventory, first: bool) -> &Inventory {
    if first { return a }
    return b
}
fn relay(value: &Inventory, depth: i32) -> &Inventory {
    if depth == 0 { return value }
    return relay(value, depth - 1)
}
fn read(value: &Inventory) -> i64 {
    let copy = *value
    if copy.enabled { return copy.counts[0] + copy.counts[3] }
    return 0
}
fn result() -> i64 {
    var first = Inventory { counts: [#(sum_to(10)), 2, 3, 5], enabled: true, unit: () }
    let second = Inventory { counts: [7, 8, 9, 10], enabled: false, unit: () }
    let borrowed = relay(choose(&first, &second, true), 3)
    let before = read(borrowed)
    first.counts[0] = 100
    let copied = first
    first.counts[1] = 8
    return before * 1000 + copied.counts[0] * 10 + first.counts[1]
}
fn main() -> i32 { if result() == 61008 { return 0 } return 1 }

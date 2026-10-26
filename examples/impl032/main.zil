struct Ledger { entries: [i64; 3]; enabled: bool; }
impl Ledger(self: &Self) {
    fn total() -> i64 => .entries[0] + .entries[1] + .entries[2]
    fn item(index: i32) -> &i64 => &.entries[index]
}
fn relay(value: &i64) -> &i64 => &*value
fn from_array(values: &[i64; 3], index: i64) -> &i64 => &values[index]
fn result() -> i64 {
    var ledger = Ledger { entries: [10, 20, 30], enabled: true }
    let picked = relay(ledger.item(1))
    let before = *picked + ledger.total()
    ledger.entries[1] = 5
    let borrowed = &ledger
    let row = &borrowed.entries
    let last = from_array(row, 2)
    return before * 100 + *last + Ledger.total(&ledger)
}
fn main() -> i32 { if result() == 8075 { return 0 } return 1 }

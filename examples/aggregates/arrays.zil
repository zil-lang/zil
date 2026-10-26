struct Item { price: i64; quantity: i64 }
fn update(input: [Item; 3]) -> [Item; 3] {
    var output = input
    for i in 0..3 {
        output[i].quantity = output[i].quantity + 1
    }
    return output
}
fn total(items: [Item; 3]) -> i64 {
    var sum: i64 = 0
    for i in 0..3 {
        sum += items[i].price * items[i].quantity
    }
    return sum
}
pub fn result() -> i64 {
    let items: [Item; 3] = [Item { price: 12, quantity: 3 }, Item { price: 7, quantity: 4 }, Item { price: 5, quantity: 2 }]
    let updated = update(items)
    return total(items) * 100 + total(updated)
}
fn main() -> i32 { if result() == 7498 { return 0 }; return 1 }

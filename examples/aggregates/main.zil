struct Item { price: i64; quantity: i64 }
struct Order { first: Item; second: Item; discount: i64 }
fn total(order: Order) -> i64 {
    return order.first.price * order.first.quantity + order.second.price * order.second.quantity - order.discount
}
fn adjust(order: Order, extra: i64) -> Order {
    var changed = order
    changed.first.quantity = changed.first.quantity + extra
    return Order { ...changed, discount: 5 }
}
pub fn result() -> i64 {
    let initial = Order {
        first: Item { price: 12, quantity: 3 },
        second: Item { price: 7, quantity: 4 },
        discount: 0
    }
    let changed = adjust(initial, 2)
    let pair: (i64, i64) = (total(initial), total(changed))
    return pair.0 * 100 + pair.1
}
fn main() -> i32 {
    if result() == 6483 { return 0 }
    return 1
}

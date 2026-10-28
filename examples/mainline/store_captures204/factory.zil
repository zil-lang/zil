import portable

resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { portable.emit(.code) } }
package Item { manages ticket: Ticket; value: i32 }
package Bag { manages items: Store<Item>; manages side: Ticket }

pub fn observer() -> impl Fn() -> i32 {
    let items: Store<Item> = store_new()
    let child = move () => { return i32(store_len(items)) }
    return move () => { return child() }
}

pub fn active(label: i32) -> impl FnMut(&mut [u8]) -> i32 {
    let items: Store<Item> = store_new()
    let bag = Bag { items: give items, side: Ticket { code: label + 32 } }
    return move (values: &mut [u8]) => {
        let prior = store_len(bag.items)
        if !store_reserve(bag.items, prior + 1) { return -1 }
        store_push(bag.items, Item {
            ticket: Ticket { code: bag.side.code - 48 }, value: i32(prior + 1)
        })
        var at: i64 = 0
        while at < values.len() {
            values[at] += 1u8
            at += 1
        }
        portable.emit(bag.side.code - 32)
        return i32(store_len(bag.items))
    }
}

pub fn once(label: i32) -> impl FnOnce() -> i32 {
    let ticket = Ticket { code: label + 32 }
    let items: Store<Item> = store_new()
    return move () => {
        portable.emit(ticket.code - 32)
        if !store_reserve(items, 1) { return -1 }
        let local = give items
        portable.emit(74)
        if !store_reserve(local, 2) { return -1 }
        store_push(local, Item {
            ticket: Ticket { code: ticket.code - 48 }, value: 1
        })
        return i32(store_len(local))
    }
}

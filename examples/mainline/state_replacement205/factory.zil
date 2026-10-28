import portable

resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { portable.emit(.code) } }
package Item { manages ticket: Ticket; value: i32 }
package Bag { manages items: Store<Item>; manages side: Ticket; counter: i32 }

pub fn active(label: i32) -> impl FnMut(i32) -> i32 {
    portable.emit(78)
    portable.emit(label)
    let items: Store<Item> = store_new()
    let bag = Bag { items: give items, side: Ticket { code: label + 32 }, counter: 0 }
    return move (step: i32) => {
        let prior = store_len(bag.items)
        if !store_reserve(bag.items, prior + 1) { return -1 }
        store_push(bag.items, Item {
            ticket: Ticket { code: bag.side.code - 48 }, value: i32(prior + 1)
        })
        bag.counter += 1
        portable.emit(bag.side.code - 32)
        return bag.counter + step
    }
}

import portable
import store_helpers

resource Mark { code: i32 }
impl Drop for Mark { fn drop(self: Self) { portable.emit(.code) } }

pub package Pair<T,F> {
    manages marker: Mark
    first: F
    second: F
    tag: T
}

package Item<T,F> {
    manages marker: Mark
    callback: F
    tag: T
}

pub fn pack<T: Copy, F: FnMut(&mut [u8]) -> i32>(
    first: F, second: F, tag: T
) -> Pair<T,F> {
    return Pair::<T,F> {
        marker: Mark { code: 90 }, first: first, second: second, tag: tag
    }
}

fn invoke<F: FnMut(&mut [u8]) -> i32>(
    callback: &mut F, values: &mut [u8], expected: i32
) -> i32 {
    let result = callback(values)
    if result < 0 { return 1 }
    if result != expected { return 8 }
    return 0
}

pub fn drive<T: Copy, G: FnMut(&mut [u8]) -> i32>(
    pair: Pair<T,G>, values: &mut [u8], count: i32
) -> void!(i32, T) {
    let tag = pair.tag
    let items: Store<Item<T,G>> = store_helpers.empty()
    var initial_capacity: i64 = 2
    if count == 6 { initial_capacity = 1 }
    if !store_helpers.reserve(items, initial_capacity) { return (1, tag) }
    store_helpers.push(items, Item::<T,G> {
        marker: Mark { code: 49 }, callback: pair.first, tag: tag
    })
    store_helpers.push(items, Item::<T,G> {
        marker: Mark { code: 50 }, callback: pair.second, tag: tag
    })
    if count == 6 {
        if store_helpers.len(items) != 1 || store_helpers.capacity(items) != 1 { return (8, tag) }
        return (0, tag)
    }
    var at = 0
    var completed = 0
    while at < count {
        if at == 1 {
            at += 1
            continue
        }
        if at == 4 { break }
        let a = store_at_mut(items, 0)
        let first = invoke(&mut a.callback, values, completed * 2 + 1)
        if first != 0 { return (first, tag) }
        let b = store_at_mut(items, 1)
        let second = invoke(&mut b.callback, values, completed * 2 + 1)
        if second != 0 { return (second, tag) }
        let reverse_b = store_at_mut(items, 1)
        let third = invoke(&mut reverse_b.callback, values, completed * 2 + 2)
        if third != 0 { return (third, tag) }
        let reverse_a = store_at_mut(items, 0)
        let fourth = invoke(&mut reverse_a.callback, values, completed * 2 + 2)
        if fourth != 0 { return (fourth, tag) }
        completed += 1
        at += 1
    }
    if !store_helpers.reserve(items, 4) {
        if store_helpers.len(items) != 2 || store_helpers.capacity(items) != 2 { return (8, tag) }
        return (1, tag)
    }
    if store_helpers.len(items) != 2 || store_helpers.capacity(items) != 4 { return (8, tag) }
    var last = store_helpers.pop(items)?
    if store_helpers.len(items) != 1 { return (8, tag) }
    if count > 0 {
        let final = invoke(&mut last.callback, values, completed * 2 + 1)
        if final != 0 { return (final, tag) }
    }
    return (0, last.tag)
}

import factory
import state
import portable
import store_helpers

fn main() -> void!i32 {
    let input = portable.read()
    if input < 48 || input > 55 { return 9 }
    let count = input - 48
    var values = [65u8, 97u8]
    let first = state.pack(factory.active(65), factory.once(66),
        portable.Tag { sequence: i64(count), valid: true }, 49)
    let second = state.pack(factory.active(67), factory.once(68),
        portable.Tag { sequence: i64(count), valid: true }, 50)
    var initial_capacity: i64 = 2
    if count == 6 { initial_capacity = 1 }
    let initial = store_helpers.identity(first)
    let items = store_helpers.seed(initial, initial_capacity)
    if store_helpers.len(items) == 0 { return 1 }
    store_helpers.push(items, second)
    if count == 6 {
        if store_helpers.len(items) != 1 || store_helpers.capacity(items) != 1 { return 8 }
        if values[0] != 65u8 || values[1] != 97u8 { return 8 }
        return 0
    }
    var at = 0
    var completed = 0
    while at < count {
        if at == 1 { at += 1; continue }
        if at == 4 { break }
        let a = store_at_mut(items, 0)
        let first_result = state.invoke(&mut a.active, &mut values[..], completed * 2 + 1)
        if first_result != 0 { return first_result }
        let c = store_at_mut(items, 1)
        let second_result = state.invoke(&mut c.active, &mut values[..], completed * 2 + 1)
        if second_result != 0 { return second_result }
        let reverse_c = store_at_mut(items, 1)
        let third_result = state.invoke(&mut reverse_c.active, &mut values[..], completed * 2 + 2)
        if third_result != 0 { return third_result }
        let reverse_a = store_at_mut(items, 0)
        let fourth_result = state.invoke(&mut reverse_a.active, &mut values[..], completed * 2 + 2)
        if fourth_result != 0 { return fourth_result }
        completed += 1
        at += 1
    }
    if !store_helpers.reserve(items, 4) {
        if store_helpers.len(items) != 2 || store_helpers.capacity(items) != 2 { return 8 }
        return 1
    }
    if store_helpers.len(items) != 2 || store_helpers.capacity(items) != 4 { return 8 }
    let popped = store_helpers.pop(items)?
    if store_helpers.len(items) != 1 { return 8 }
    let transferred = store_helpers.identity(popped)
    let response = state.finish(transferred, &mut values[..], completed * 2 + 1)
    if response.1.sequence != i64(count) || !response.1.valid { return 8 }
    if response.0 != 0 { return response.0 }
    if values[0] != u8(65 + 4 * completed + 1) || values[1] != u8(97 + 4 * completed + 1) { return 8 }
    return 0
}

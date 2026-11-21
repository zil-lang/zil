import factory
import state
import portable
import store_helpers

fn main() -> void!i32 {
    let input = portable.read()
    if input < 48 || input > 55 { return 9 }
    let mode = input - 48
    let initial = state.pack(factory.active(65), factory.active(66),
        portable.Tag { sequence: i64(mode), valid: true })
    let items = store_helpers.seed(store_helpers.identity(initial), 1)
    if store_helpers.len(items) != 1 { return 1 }

    var iterations = 0
    if mode == 1 || mode == 2 { iterations = 1 }
    if mode == 7 { iterations = 7 }
    var at = 0
    var completed = 0
    while at < iterations {
        if at == 1 { at += 1; continue }
        if at == 4 { break }
        let first = store_at_mut(items, 0)
        let result_a = state.invoke(&mut first.a, completed + 1)
        if result_a != 0 { return result_a }
        let second = store_at_mut(items, 0)
        let result_b = state.invoke(&mut second.b, completed + 1)
        if result_b != 0 { return result_b }
        completed += 1
        at += 1
    }

    let owned = store_helpers.pop(items)?
    if store_helpers.len(items) != 0 { return 8 }
    let guard = portable.Guard { code: 90 }
    if mode == 3 {
        let unused = state.replace(owned, factory.active(portable.fail_label()?), false)
        return 8
    }
    if mode == 4 {
        let unused = state.replace(owned, factory.active(67), portable.fail_flag()?)
        return 8
    }
    if mode == 5 {
        let unused = state.replace_first(factory.active(67), i64(42), owned, portable.fail_flag()?)
        return 8
    }

    var replacement = factory.active(67)
    let prepared = replacement(0)
    if prepared < 0 { return 1 }
    if prepared != 1 { return 8 }
    if mode == 0 {
        let updated = state.replace_first(replacement, i64(42), owned, false)
        store_helpers.push(items, updated)
    } else if mode == 6 {
        let updated = state.consume_replace(owned, replacement)
        store_helpers.push(items, updated)
    } else {
        let updated = state.replace(owned, replacement, mode == 2)
        store_helpers.push(items, updated)
    }
    if store_helpers.len(items) != 1 { return 8 }
    let saved = store_at(items, 0)
    if saved.tag.sequence != i64(mode) || !saved.tag.valid { return 8 }
    var expected_a = 2
    if mode == 2 { expected_a = completed + 1 }
    let changed = store_at_mut(items, 0)
    let result_a = state.invoke(&mut changed.a, expected_a)
    if result_a != 0 { return result_a }
    let sibling = store_at_mut(items, 0)
    let result_b = state.invoke(&mut sibling.b, completed + 1)
    if result_b != 0 { return result_b }
    let repeated = store_at_mut(items, 0)
    let result_again = state.invoke(&mut repeated.a, expected_a + 1)
    if result_again != 0 { return result_again }

    if !store_helpers.reserve(items, 2) {
        if store_helpers.len(items) != 1 || store_helpers.capacity(items) != 1 { return 8 }
        return 1
    }
    if store_helpers.len(items) != 1 || store_helpers.capacity(items) != 2 { return 8 }
    portable.emit(76)
    return 0
}

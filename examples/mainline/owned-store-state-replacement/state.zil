import model
import storage

pub package State { manages ticket: model.Ticket; manages children: Store<model.Item>; pub revision: i32 }

pub fn make() -> void!State {
    let ticket = model.Ticket { code: 90 }
    let children = storage.into_store(storage.prepare(2, 65)?)
    return State { ticket: give ticket, children: give children, revision: 0 }
}

pub fn replace(input: State, count: i32) -> void!State {
    var current = input
    current.children = storage.into_store(storage.prepare(count, 67)?)
    if store_len(current.children) != i64(count) { return () }
    if count > 0 { storage.check_tail(current.children, 66 + count)? }
    current.revision = 1
    return current
}

pub fn relay(input: State) -> State { return input }

pub fn take_then_error(input: State) -> void!i32 {
    let taken = give input.children
    return ()
}

pub fn reserve_recovery(input: State) -> void!i32 {
    let capacity = store_capacity(input.children)
    let grown = store_reserve(input.children, 8)
    if !grown && store_capacity(input.children) != capacity { return () }
    if store_len(input.children) != 2 { return () }
    storage.check_tail(input.children, 66)?
    if store_len(input.children) != 2 { return () }
    return 0
}

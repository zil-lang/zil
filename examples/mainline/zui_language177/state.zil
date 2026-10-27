import model
import collection

pub package State<F> {
    manages ticket: model.Ticket
    manages children: Store<model.Item>
    pub callback: F
    pub hits: i32
    pub sum: i32
    pub ready: bool
}

pub fn create<F: FnMut() -> i32>(callback: F, count: i32) -> State<F> {
    let ticket = model.Ticket { code: 90 }
    let children: Store<model.Item> = store_new()
    let ready = collection.fill(children, count, 68)
    let total = collection.sum(children)
    return State<F> { ticket: give ticket, children: give children, callback: callback, hits: 0, sum: total, ready: ready }
}

pub fn tick<G: FnMut() -> i32>(state: State<G>) -> State<G> {
    var next = state
    let callback = &mut next.callback
    next.hits = callback()
    return next
}

pub fn replace_children<F: FnMut() -> i32>(state: State<F>, count: i32) -> State<F> {
    var next = state
    let replacement: Store<model.Item> = store_new()
    if !collection.fill(replacement, count, 77) {
        next.ready = false
        next.sum = collection.sum(next.children)
        return next
    }
    next.children = give replacement
    next.sum = collection.sum(next.children)
    next.ready = true
    return next
}

pub fn grow<F: FnMut() -> i32>(state: State<F>, capacity: i64) -> State<F> {
    var next = state
    let old_len = store_len(next.children)
    let old_capacity = store_capacity(next.children)
    let old_sum = collection.sum(next.children)
    let grown = store_reserve(next.children, capacity)
    next.ready = store_len(next.children) == old_len
    if !grown && store_capacity(next.children) != old_capacity { next.ready = false }
    next.sum = collection.sum(next.children)
    if next.sum != old_sum { next.ready = false }
    return next
}

pub fn take_then_exit<F: FnMut() -> i32>(state: State<F>) -> i32 {
    var current = state
    var callback = current.callback
    let children = give current.children
    if collection.sum(children) != current.sum { return 2 }
    if callback() != 1 || callback() != 2 { return 3 }
    return 1
}

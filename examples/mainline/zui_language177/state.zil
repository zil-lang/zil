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

package Batch {
    manages items: Store<model.Item>
    ready: bool
    sum: i32
}

fn prepare(count: i32, start: i32) -> Batch {
    let items = collection.empty()
    let ready = collection.fill(items, count, start)
    let total = collection.sum(items)
    return Batch { items: give items, ready: ready, sum: total }
}

pub fn create<F: FnMut() -> i32>(callback: F, count: i32) -> State<F> {
    let prepared = prepare(count, 68)
    return State::<F> { ticket: model.Ticket { code: 90 }, children: give prepared.items, callback: callback, hits: 0, sum: prepared.sum, ready: prepared.ready }
}

pub fn tick<G: FnMut() -> i32>(state: State<G>) -> State<G> {
    var next = state
    let callback = &mut next.callback
    next.hits = callback()
    return next
}

pub fn replace_children<F: FnMut() -> i32>(state: State<F>, count: i32) -> State<F> {
    var next = state
    let replacement = prepare(count, 77)
    if !replacement.ready {
        next.ready = false
        next.sum = collection.sum(next.children)
        return next
    }
    next.children = give replacement.items
    next.sum = collection.sum(next.children)
    next.ready = true
    return next
}

pub fn grow<F: FnMut() -> i32>(state: State<F>, capacity: i64) -> State<F> {
    var next = state
    next.ready = collection.grow(next.children, capacity)
    next.sum = collection.sum(next.children)
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

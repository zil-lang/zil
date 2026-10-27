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
    let children: Store<model.Item> = collection.empty()
    let ready = collection.fill(children, count, 68)
    let total = collection.sum(children)
    return State::<F> { ticket: give ticket, children: give children, callback: callback, hits: 0, sum: total, ready: ready }
}

pub fn tick<G: FnMut() -> i32>(state: State<G>) -> State<G> {
    var next = state
    let callback = &mut next.callback
    next.hits = callback()
    return next
}

pub fn replace_children<F: FnMut() -> i32>(state: State<F>, count: i32) -> State<F> {
    var next = state
    let replacement: Store<model.Item> = collection.empty()
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

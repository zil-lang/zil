import model

pub type Event = model.Cell<i64>;

pub fn initialize<F: Fn(Event) -> i64>(f: &F, n: Event) -> i64 {
    return f(n);
}

pub fn warm<F: FnMut(Event) -> i64>(f: &mut F) -> i64 {
    return f(model.pack(1)) + f(model.pack(2)) + f(model.pack(3)) + f(model.pack(4));
}

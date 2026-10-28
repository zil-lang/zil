pub package State<T,F> {
    manages storage: ByteBuffer
    first: F
    second: F
    tag: T
    ready: bool
}

pub fn pack<T: Copy, F: FnMut(&mut [u8]) -> i32>(
    first: F, second: F, tag: T
) -> State<T,F> {
    let storage = bytes_new()
    let ready = bytes_reserve(storage, 1)
    return State::<T,F> {
        storage: give storage, first: first, second: second, tag: tag, ready: ready
    }
}

fn relay<T: Copy, G: FnMut(&mut [u8]) -> i32>(state: State<T,G>) -> State<T,G> {
    return state
}

fn both<F: FnMut(&mut [u8]) -> i32>(
    first: &mut F, second: &mut F, values: &mut [u8], expected: i32
) -> i32 {
    let a = first(values)
    if a < 0 { return 1 }
    if a != expected { return 8 }
    let b = second(values)
    if b < 0 { return 1 }
    if b != expected { return 8 }
    return 0
}

pub fn drive<T: Copy, G: FnMut(&mut [u8]) -> i32>(
    state: State<T,G>, values: &mut [u8], count: i32
) -> (i32, T) {
    var current = relay(state)
    if !current.ready { return (1, current.tag) }
    var at = 0
    var completed = 0
    while at < count {
        if at == 1 {
            at += 1
            continue
        }
        if at == 4 { break }
        let forward = both(&mut current.first, &mut current.second, values, completed * 2 + 1)
        if forward != 0 { return (forward, current.tag) }
        let reverse = both(&mut current.second, &mut current.first, values, completed * 2 + 2)
        if reverse != 0 { return (reverse, current.tag) }
        completed += 1
        at += 1
    }
    return (0, current.tag)
}

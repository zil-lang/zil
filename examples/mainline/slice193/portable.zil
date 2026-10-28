import helpers

struct Row { byte: u8, wide: i64 }
fn discard(give buffer: ByteBuffer) {}
fn sum(values: &[u8]) -> i64 {
    var result: i64 = 0
    var at: i64 = 0
    while at < values.len() { result += i64(values[at]); at += 1 }
    return result
}
fn source(values: &[i32], order: &mut i32) -> &[i32] {
    *order = *order * 10 + 1
    return values
}
fn edge(order: &mut i32, digit: i32, index: i32) -> i32 {
    *order = *order * 10 + digit
    return index
}

pub fn check() -> i32 {
    // Six fat windows exceed the native integer-register argument count.
    let a = [1u8]
    let b = [2u8, 3u8]
    let c = [4u8, 5u8, 6u8]
    let d = [7u8, 8u8, 9u8, 10u8]
    let e = [11u8, 12u8, 13u8, 14u8, 15u8]
    let f = [16u8, 17u8, 18u8, 19u8, 20u8, 21u8]
    let six = (a: &[u8], b: &[u8], c: &[u8], d: &[u8], e: &[u8], f: &[u8]) => {
        return sum(a) + sum(b) + sum(c) + sum(d) + sum(e) + sum(f) +
            a.len() + b.len() + c.len() + d.len() + e.len() + f.len()
    }
    if six(&a[..], &b[..], &c[..], &d[..], &e[..], &f[..]) != 252 { return 11 }

    var rows = [Row { byte: 1u8, wide: 2 }, Row { byte: 3u8, wide: 4 }]
    let modify = (values: &mut [Row]) => { values[1].wide = 41; return values[1].wide }
    if modify(&mut rows[..]) != 41 || rows[0].byte != 1u8 { return 12 }
    var units = [(), (), ()]
    let unit = (values: &mut [()]) => { values[0] = (); return values.len() }
    if unit(&mut units[1..]) != 2 { return 13 }
    let empty: [i32; 0] = []
    let length = (values: &[i32]) => values.len()
    if length(&empty[..]) != 0 { return 14 }

    var order = 0
    let integers = [4, 7, 9, 12]
    let inspect = (values: &[i32]) => values[0] + i32(values.len())
    if inspect(&source(&integers[..], &mut order)[edge(&mut order, 2, 1)..edge(&mut order, 3, 3)]) != 9 {
        return 15
    }
    if order != 123 { return 16 }

    var values = [40u8, 1u8]
    let view = &mut values[..]
    let read = (input: &[u8]) => i32(input[0]) + i32(input[1])
    if helpers.read(&read, view) != 41 { return 17 }
    let bump = (input: &mut [u8]) => { input[0] += input[1]; return i32(input[0]) }
    if bump(&mut *view) != 41 || bump(&mut *view) != 42 { return 18 }
    let constant = () => 1
    if helpers.extra(&constant, &values[..]) != 43 { return 19 }

    // Consuming an empty owned buffer makes the concrete environment FnOnce
    // without introducing an allocation into this ABI control.
    let owned = bytes_new()
    let once = move (input: &mut [u8]) => {
        discard(give owned)
        input[0] += 1u8
        return i32(input[0])
    }
    if helpers.consume(once, &mut values[..]) != 43 { return 20 }
    return 0
}

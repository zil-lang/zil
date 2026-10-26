import sequence

// More than the native integer-register argument count: ordinary internal
// slice calling convention must preserve every pointer/length on both targets.
fn six(a: &[u8], b: &[u8], c: &[u8], d: &[u8], e: &[u8], f: &[u8]) -> i64 {
    return sequence.sum(a) + sequence.sum(b) + sequence.sum(c) +
        sequence.sum(d) + sequence.sum(e) + sequence.sum(f) +
        a.len() + b.len() + c.len() + d.len() + e.len() + f.len()
}
fn main() -> i32 {
    var first = [97u8, 98u8, 99u8, 100u8]
    let suffix = sequence.mutable_tail(&mut first[..], 1)
    sequence.uppercase_ascii(suffix)
    if first[0] != 97u8 || first[1] != 66u8 || first[3] != 68u8 { return 1 }
    let a = [1u8]
    let b = [2u8, 3u8]
    let c = [4u8, 5u8, 6u8]
    let d = [7u8, 8u8, 9u8, 10u8]
    let e = [11u8, 12u8, 13u8, 14u8, 15u8]
    let f = [16u8, 17u8, 18u8, 19u8, 20u8, 21u8]
    if six(&a[..], &b[..], &c[..], &d[..], &e[..], &f[..]) != 252 { return 2 }
    let tail = sequence.relay(&first[..], 2)
    if tail.len() != 2 || sequence.sum(tail) != 135 { return 3 }
    var unit = [(), (), ()]
    let zero = &mut unit[1..]
    zero[0] = ()
    if zero.len() != 2 { return 4 }
    let empty: [i32; 0] = []
    let window = &empty[..]
    if window.len() != 0 { return 5 }
    return 0
}

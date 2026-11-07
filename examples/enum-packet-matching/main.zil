enum Packet {
    Empty,
    Count(i32),
    Pair(i64, bool),
}
fn make(n: i32) -> Packet {
    if n == 0 { return Packet.Empty }
    if n < 3 { return Packet.Count(n) }
    return Packet.Pair(5000000000, true)
}
fn score(packet: Packet) -> i64 {
    match packet {
        Packet.Empty() => { return 7 },
        Packet.Count(0) => { return 100 },
        Packet.Count(n) => { return 40 },
        Packet.Pair(value, true) => { return value + 2 },
        Packet.Pair(value, false) => { return value },
    }
}
fn answer() -> i64 {
    return score(make(0)) + score(make(2)) + score(make(5))
}
fn main() -> i32 {
    if answer() == 5000000049 { return 0 }
    return 1
}

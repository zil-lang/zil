#[derive(Clone, Copy)]
struct Packet {
    sequence: i64,
    samples: [i64; 2],
    active: bool,
}
fn some<T>(value: T) -> Option<T> {
    Some(value)
}
fn empty<T>() -> Option<T> {
    None
}
fn success<E, T>(value: T) -> Result<T, E> {
    Ok(value)
}
fn select<T>(value: Option<Result<T, bool>>, fallback: T) -> T {
    match value {
        None | Some(Err(_)) => fallback,
        Some(Ok(payload)) => payload,
    }
}
fn score(packet: Packet) -> i64 {
    if packet.active {
        packet.sequence + packet.samples[0] + packet.samples[1]
    } else {
        0
    }
}
fn answer() -> i64 {
    let fallback = Packet {
        sequence: 0,
        samples: [0, 0],
        active: false,
    };
    let live: Option<Result<Packet, bool>> = some(success(Packet {
        sequence: 5_000_000_000,
        samples: [40, 2],
        active: true,
    }));
    let failed: Option<Result<Packet, bool>> = some(Err(true));
    let missing: Option<Result<Packet, bool>> = empty();
    score(select(live, fallback))
        + score(select(failed, fallback))
        + score(select(missing, fallback))
}
fn main() {
    println!("{}", answer());
}

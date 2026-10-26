// User-defined enum families; no compiler-special Option or Result names.
enum Option<T> { None, Some(T) }
enum Result<E, T> { Err(E), Ok(T) }
struct Packet {
    sequence: i64
    samples: [i64; 2]
    active: bool
}
fn some<T>(value: T) -> Option<T> { return Option.Some(value) }
fn empty<T>() -> Option<T> { return Option.None }
fn success<E, T>(value: T) -> Result<E, T> { return Result.Ok(value) }
fn select<T>(value: Option<Result<bool, T>>, fallback: T) -> T {
    match value {
        Option.None() => { return fallback }
        Option.Some(Result.Err(_)) => { return fallback }
        Option.Some(Result.Ok(payload)) => { return payload }
    }
}
fn score(packet: Packet) -> i64 {
    if packet.active {
        return packet.sequence + packet.samples[0] + packet.samples[1]
    }
    return 0
}
pub fn answer() -> i64 {
    let fallback = Packet { sequence: 0, samples: [0, 0], active: false }
    let live: Option<Result<bool, Packet>> = some(success(Packet {
        sequence: 5000000000, samples: [40, 2], active: true
    }))
    let failed: Option<Result<bool, Packet>> = some(Result.Err(true))
    let missing: Option<Result<bool, Packet>> = empty()
    return score(select(live, fallback)) + score(select(failed, fallback)) + score(select(missing, fallback))
}
fn main() -> i32 {
    if answer() == 5000000042 { return 0 }
    return 1
}

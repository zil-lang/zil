// User-defined enum families; no compiler-special LegacyOption or Result names.
enum LegacyOption<T> { None, Some(T) }
enum Result<E, T> { Err(E), Ok(T) }
struct Packet {
    sequence: i64
    samples: [i64; 2]
    active: bool
}
fn some<T>(value: T) -> LegacyOption<T> { return LegacyOption.Some(value) }
fn empty<T>() -> LegacyOption<T> { return LegacyOption.None }
fn success<E, T>(value: T) -> Result<E, T> { return Result.Ok(value) }
fn select<T>(value: LegacyOption<Result<bool, T>>, fallback: T) -> T {
    match value {
        LegacyOption.None() => { return fallback }
        LegacyOption.Some(Result.Err(_)) => { return fallback }
        LegacyOption.Some(Result.Ok(payload)) => { return payload }
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
    let live: LegacyOption<Result<bool, Packet>> = some(success(Packet {
        sequence: 5000000000, samples: [40, 2], active: true
    }))
    let failed: LegacyOption<Result<bool, Packet>> = some(Result.Err(true))
    let missing: LegacyOption<Result<bool, Packet>> = empty()
    return score(select(live, fallback)) + score(select(failed, fallback)) + score(select(missing, fallback))
}
fn main() -> i32 {
    if answer() == 5000000042 { return 0 }
    return 1
}

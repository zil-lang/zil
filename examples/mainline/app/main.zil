import seed.data as seed
import ops

// These are ordinary user declarations, with no privileged Option/Result names.
enum Option<T> { None, Some(T) }
enum Result<E, T> { Err(E), Ok(T) }
struct Row<T> { index: i64; value: T }
struct Stats { accepted: i64; rejected: i64; total: i64 }

fn next(values: &[i64; 8], index: i64) -> Option<Row<i64>> {
    if index >= 8 { return Option.None }
    return Option.Some(Row::<i64> { index: index, value: *ops.at(values, index) })
}

fn validate(row: Row<i64>) -> Result<i64, Row<i64>> {
    if row.value < 0 { return Result.Err(row.index) }
    return Result.Ok(row)
}

pub fn result() -> i64 {
    let data = #(seed.build())
    let weights = #(seed.weights())
    let callbacks: [fn(i64) -> i64; 2] = [ops.double, ops.offset]
    let apply = (f, x) => f(x)

    var stats = Stats { accepted: 0, rejected: 0, total: 0 }
    var record = move (accepted: bool, value: i64) => {
        if accepted {
            stats.accepted += 1
            stats.total += value
        } else {
            stats.rejected += 1
        }
        stats
    }

    var index: i64 = 0
    var errors: i64 = 0
    while let Option.Some(row) = next(&data.values, index) {
        index += 1
        let checked = validate(row)
        if let Result.Err(code) = checked {
            errors += code
            record(false, code)
            continue
        }
        if let Result.Ok(valid) = checked {
            let transformed = apply(callbacks[valid.index % 2], valid.value)
            record(true, transformed)
        }
    }

    // The Copy closure owns a separate Stats value after this assignment.
    var fork = record
    record(true, apply(callbacks[0], weights[0]))
    let forked = fork(true, apply(callbacks[1], weights[1]))
    let final_state = record(false, 0)
    let shared = &final_state
    let total = ops.relay(&shared.total)
    return (*total * 10 + forked.total + *seed.checksum(&data)
        + *ops.at(&data.values, 1) + final_state.accepted * 100000
        + final_state.rejected * 10000 + forked.accepted * 1000
        + forked.rejected * 100 + errors * 10 + stats.total)
}

fn main() -> i32 {
    if result() == 475000738663 { return 0 }
    return 1
}

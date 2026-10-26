mod ops;
mod seed;
use seed::data;

#[derive(Clone, Copy)]
struct Row<T> {
    index: i64,
    value: T,
}

#[derive(Clone, Copy)]
struct Stats {
    accepted: i64,
    rejected: i64,
    total: i64,
}

fn next(values: &[i64; 8], index: i64) -> Option<Row<i64>> {
    if index >= 8 {
        return None;
    }
    Some(Row {
        index,
        value: *ops::at(values, index),
    })
}

fn validate(row: Row<i64>) -> Result<Row<i64>, i64> {
    if row.value < 0 {
        return Err(row.index);
    }
    Ok(row)
}

fn result() -> i64 {
    const DATA: data::Dataset = data::build();
    const WEIGHTS: [i64; 2] = data::weights();
    let data = DATA;
    let weights = WEIGHTS;
    let callbacks: [fn(i64) -> i64; 2] = [ops::double, ops::offset];
    let apply = |f: fn(i64) -> i64, x| f(x);

    let mut stats = Stats {
        accepted: 0,
        rejected: 0,
        total: 0,
    };
    let mut record = move |accepted: bool, value: i64| {
        if accepted {
            stats.accepted += 1;
            stats.total += value;
        } else {
            stats.rejected += 1;
        }
        stats
    };

    let mut index = 0;
    let mut errors = 0;
    while let Some(row) = next(&data.values, index) {
        index += 1;
        let checked = validate(row);
        if let Err(code) = checked {
            errors += code;
            record(false, code);
            continue;
        }
        if let Ok(valid) = checked {
            let transformed = apply(callbacks[(valid.index % 2) as usize], valid.value);
            record(true, transformed);
        }
    }

    let mut fork = record;
    record(true, apply(callbacks[0], weights[0]));
    let forked = fork(true, apply(callbacks[1], weights[1]));
    let final_state = record(false, 0);
    let shared = &final_state;
    let total = ops::relay(&shared.total);
    *total * 10
        + forked.total
        + *data::checksum(&data)
        + *ops::at(&data.values, 1)
        + final_state.accepted * 100_000
        + final_state.rejected * 10_000
        + forked.accepted * 1_000
        + forked.rejected * 100
        + errors * 10
        + stats.total
}

fn main() {
    let value = result();
    assert_eq!(value, 475_000_738_663);
    // This wrapper matches ZIL's selected-result printer and ordinary main.
    if std::env::args().nth(1).as_deref() != Some("--check") {
        println!("{value}");
    }
}

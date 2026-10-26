import facade as f

pub fn wrapped(value: i64) -> f.IntOptional {
    if value == 0 { return f.Optional::<f.IntEntry>.None }
    return f.Optional::<f.IntEntry>.Some(f.make(value))
}

pub fn score(value: f.IntOptional) -> i64 {
    match value {
        f.IntOptional.None() => { return 0 },
        f.IntOptional.Some(item) => { return item.value * item.weight }
    }
}

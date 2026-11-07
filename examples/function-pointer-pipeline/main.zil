struct Sample { value: i64; }
struct Pipeline {
    sample: fn(Sample) -> Sample;
    transforms: [fn(i64) -> i64; 2];
}

fn apply(f: fn(i64) -> i64, value: i64) -> i64 => f(value)
fn choose(flag: bool) -> fn(i64) -> i64 => if flag {x => x + 1} else {x => x * 2}
fn make() -> Pipeline => Pipeline {
    sample: s => Sample {value: s.value + 3},
    transforms: [n => n + 2, n => n * 2],
}

fn result() -> i64 {
    let pipeline = make()
    let sample = pipeline.sample(Sample {value: 5000000000})
    var f: fn(i64) -> i64 = choose(true)
    let first = f(sample.value)
    f = x => x + 10
    let tail: (fn(i64) -> i64, fn(i64) -> i64) = (x => x + 1, x => x + 2)
    return (first + pipeline.transforms[0](10) + pipeline.transforms[1](5)
        + apply(x => x + 3, 2) + tail.0(1) + tail.1(2) + f(0) - 5)
}

fn main() {}

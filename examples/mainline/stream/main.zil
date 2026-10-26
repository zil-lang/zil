import io
import model
import tokens
import transform

fn main() -> i32 {
    let callbacks: [fn(i32) -> i32; 2] = [transform.double, transform.offset]
    var stats = model.empty()
    var record = (item: model.Result<i32, i32>) => {
        if let model.Result.Ok(value) = item {
            let transform = callbacks[stats.accepted % 2]
            stats.total += transform(value)
            stats.accepted += 1
        } else {
            stats.rejected += 1
        }
    }

    var count: i32 = 0
    while let model.Option.Some(item) = tokens.next() {
        if count >= 1000 { return 2 }
        record(model.identity(item))
        count += 1
    }

    if !io.number(stats.accepted) { return 3 }
    if io.write(32) < 0 { return 3 }
    if !io.number(stats.rejected) { return 3 }
    if io.write(32) < 0 { return 3 }
    if !io.number(stats.total) { return 3 }
    if io.write(10) < 0 { return 3 }
    return 0
}

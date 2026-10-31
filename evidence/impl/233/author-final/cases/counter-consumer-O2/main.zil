import operations
import counters

fn read<T>(value: &T) -> i32 where T: operations.Counter => value.get()

fn main() -> i32 {
    var first = counters.make(10)
    first.set(20)
    let second = counters.make(2)
    if read(&first) != 20 { return 1 }
    if first.plus(&second) != 22 { return 2 }
    return second.finish() - 2
}

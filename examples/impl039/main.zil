type Number = i64
type Row = Cell<Number>
type Maybe = Option<Row>

enum Option<T> { None, Some(T) }
struct Cell<T> { value: T; }

fn next(index: Number) -> Maybe {
    if index < 3 { return Maybe.Some(Row { value: index + 13 }) }
    return Maybe.None
}
fn result() -> Number {
    var index: Number = 0
    var sum: Number = 0
    while let Maybe.Some(row) = next(index) {
        sum += row.value
        index += 1
    }
    return sum
}
fn main() -> i32 {
    if result() == 42 { return 0 }
    return 1
}

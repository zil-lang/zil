type Number = i64;
type Row = Cell<Number>;
type Maybe = Option<Row>;
struct Cell<T> { value: T }
fn next(index: Number) -> Maybe {
    if index < 3 { Some(Row { value: index + 13 }) } else { None }
}
fn main() {
    let mut index: Number = 0;
    let mut sum: Number = 0;
    while let Some(row) = next(index) {
        sum += row.value;
        index += 1;
    }
    println!("{sum}");
}

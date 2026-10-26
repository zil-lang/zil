import model

// Aliases keep model's nominal identity. They introduce no new runtime layout.
pub type Entry<T> = model.Item<T>
pub type Optional<T> = model.Maybe<T>
pub type Table<T> = (Entry<T>, Entry<T>)
pub type IntEntry = Entry<i64>
pub type IntOptional = Optional<IntEntry>

pub fn make(value: i64) -> Entry<i64> {
    return Entry::<i64> { value: model.identity(value), weight: 2 }
}

pub fn table() -> Table<i64> {
    return (Entry::<i64> { value: 3, weight: 4 },
            Entry::<i64> { value: 5, weight: 6 })
}

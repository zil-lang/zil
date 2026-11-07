pub enum Option<T> { None, Some(T) }
pub enum Result<E, T> { Err(E), Ok(T) }
pub struct Row<T> { pub value: T; pub weight: i64 }
pub struct Secret { value: i64; pub visible: i64 }

pub type Count = i64
pub type Score = Result<bool, Count>
pub type MaybeRow = Option<Row<Count>>

pub fn some<T>(value: T) -> Option<T> { return Option.Some(value) }
pub fn empty<T>() -> Option<T> { return Option.None }
pub fn ok<E, T>(value: T) -> Result<E, T> { return Result.Ok(value) }
pub fn unwrap<T>(value: Option<T>, fallback: T) -> T {
    match value {
        Option.Some(v) => { return v }
        Option.None() => { return fallback }
    }
}
pub fn secret(value: i64) -> Secret { return Secret { value: value, visible: 2 } }
pub fn read(value: Secret) -> i64 { return value.value }

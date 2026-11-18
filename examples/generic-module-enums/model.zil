pub enum LegacyOption<T> { None, Some(T) }
pub enum Result<E, T> { Err(E), Ok(T) }
pub struct Row<T> { pub value: T; pub weight: i64 }
pub struct Secret { value: i64; pub visible: i64 }

pub type Count = i64
pub type Score = Result<bool, Count>
pub type MaybeRow = LegacyOption<Row<Count>>

pub fn some<T: Value>(value: T) -> LegacyOption<T> { return LegacyOption.Some(value) }
pub fn empty<T: Value>() -> LegacyOption<T> { return LegacyOption.None }
pub fn ok<E: Value, T: Value>(value: T) -> Result<E, T> { return Result.Ok(value) }
pub fn unwrap<T: Value+SyncDrop>(value: LegacyOption<T>, fallback: T) -> T {
    match value {
        LegacyOption.Some(v) => { return v }
        LegacyOption.None() => { return fallback }
    }
}
pub fn secret(value: i64) -> Secret { return Secret { value: value, visible: 2 } }
pub fn read(value: Secret) -> i64 { return value.value }

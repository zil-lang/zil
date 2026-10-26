pub struct Item<T> { pub value: T; pub weight: i64 }
pub enum Maybe<T> { None, Some(T) }

pub fn identity<T>(value: T) -> T { return value }

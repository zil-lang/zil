mod model {
    pub enum Option<T> {
        None,
        Some(T),
    }
    pub enum Result<E, T> {
        Err(E),
        Ok(T),
    }
    pub struct Row<T> {
        pub value: T,
        pub weight: i64,
    }
    pub struct Secret {
        value: i64,
        pub visible: i64,
    }
    pub type Count = i64;
    pub type Score = Result<bool, Count>;
    pub type MaybeRow = Option<Row<Count>>;
    pub fn some<T>(value: T) -> Option<T> {
        Option::Some(value)
    }
    pub fn empty<T>() -> Option<T> {
        Option::None
    }
    pub fn ok<E, T>(value: T) -> Result<E, T> {
        Result::Ok(value)
    }
    pub fn unwrap<T>(value: Option<T>, fallback: T) -> T {
        match value {
            Option::Some(v) => v,
            Option::None => fallback,
        }
    }
    pub const fn secret(value: i64) -> Secret {
        Secret { value, visible: 2 }
    }
    pub fn read(value: Secret) -> i64 {
        value.value
    }
}
mod process {
    use super::model as m;
    pub fn convert(value: m::MaybeRow) -> m::Score {
        if let m::Option::Some(row) = value {
            return m::ok(row.value + row.weight);
        }
        m::Result::Err(true)
    }
}
fn result() -> i64 {
    let row = model::Row::<i64> {
        value: 5_000_000_000,
        weight: 40,
    };
    let value: model::MaybeRow = model::some(row);
    let score = process::convert(value);
    let unused: model::Option<i64> = model::empty();
    const OPAQUE: model::Secret = model::secret(0);
    let opaque = OPAQUE;
    let extra = model::unwrap(unused, opaque.visible) + model::read(opaque);
    match score {
        model::Score::Ok(n) => n + extra,
        model::Score::Err(_) => 0,
    }
}
fn main() {
    println!("{}", result());
}

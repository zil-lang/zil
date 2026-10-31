pub trait Counter {
    fn get(self: &Self) -> i32;
    fn set(self: &mut Self, n: i32);
    fn plus(self: &Self, other: &Self) -> i32;
    fn make(n: i32) -> Self;
    fn finish(self: Self) -> i32;
}

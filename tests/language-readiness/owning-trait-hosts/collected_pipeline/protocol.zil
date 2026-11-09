pub enum Error { Allocation, Requested }
pub trait LendingIterator {
    type Item;
    fn next<life a>(self:&'a mut Self)->Option<&'a Self.Item>;
}
pub trait Tag { fn tag(self:&Self)->i64; }
pub trait Duplicate {
    fn duplicate(self:&Self)->Self;
    fn try_duplicate(self:&Self,fail:bool)->Error!Self;
}

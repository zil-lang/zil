pub enum Error { Allocation, Requested }
pub trait Duplicate {
    fn duplicate(self:&Self)->Self;
    fn try_duplicate(self:&Self,fail:bool)->Error!Self;
}

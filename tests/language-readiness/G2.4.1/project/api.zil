pub trait LendingIterator {
    type Item:Copy;
    fn next<life a>(self:&'a mut Self)->Option<&'a Self.Item>;
}

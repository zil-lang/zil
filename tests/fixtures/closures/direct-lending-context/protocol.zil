pub trait LendingIterator {type Item;fn next<life a>(self:&'a mut Self)->Option<&'a Self.Item>;}

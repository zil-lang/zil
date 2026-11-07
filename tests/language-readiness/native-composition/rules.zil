pub trait Read {
    type Item:Copy;
    fn read(self:&Self)->Self.Item;
}

pub fn get<T:Read>(value:&T)->T.Item { return value.read(); }

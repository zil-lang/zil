pub trait Marker<T:Copy> {}

pub trait Read<T:Copy> {
    type Item:Copy;
    fn read(self:&Self)->Self.Item;
}

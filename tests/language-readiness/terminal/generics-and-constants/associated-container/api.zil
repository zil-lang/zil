pub trait Read { type Item:Copy; fn read(self:&Self)->Self.Item; }

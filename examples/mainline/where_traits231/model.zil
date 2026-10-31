pub trait Read { fn get(self:&Self)->i32; }

pub struct Counter { n:i32 }

impl Read for Counter { fn get()->i32=>.n }

pub fn make(n:i32)->Counter=>Counter{n:n}

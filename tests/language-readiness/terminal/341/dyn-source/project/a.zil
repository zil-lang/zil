pub trait Read{fn read(self:&Self)->i32;}
pub struct Meter{n:i32}
impl Read for Meter{fn read(self:&Self)->i32{return self.n}}

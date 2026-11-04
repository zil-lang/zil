pub trait Read{fn read(self:&Self)->i32{return 9}}
pub struct Meter{n:i32}
impl Read for Meter{}

pub trait Read { fn read(self:&Self)->i32; }
pub struct Meter { pub n:i32 }
impl Read for Meter { fn read(self:&Self)->i32 { return self.n } }
pub fn read_it(x:&dyn Read)->i32 { return x.read() }

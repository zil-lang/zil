extern "C" fn putchar(x:i32)->i32;
pub trait Read{fn read(self:&Self)->i32;}
pub resource R{pub n:i32}
impl Read for R{pub fn read()->i32=>.n}
impl Drop for R{fn drop(self:Self){unsafe{putchar(65)}}}
pub fn make(n:i32)->R contract{result(create)}{return R{n:n}}
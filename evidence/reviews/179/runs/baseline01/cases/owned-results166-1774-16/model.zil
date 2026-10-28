
extern "C" fn putchar(x:i32)->i32;
pub resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
pub package P{manages r:R}
pub enum E{Fail}
pub fn make()->E!P=>P{r:R{code:65}}
pub fn read(p:P)->i32=>p.r.code


extern "C" fn putchar(x:i32)->i32;
struct Data{small:u8;n:i32;lanes:[i32;2]}
resource R{code:i32;data:Data}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn inspect(h:&R)->i32{let p=&h.data.n;return *p+h.data.lanes[1]}
fn main()->i32{let r=R{code:65,data:Data{small:u8(1),n:20,lanes:[2,22]}};return inspect(&r)-42}

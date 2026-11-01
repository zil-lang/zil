extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){emit(.code)}}fn main()->i32{defer{emit(90)}for i in 0..3{if i==1{continue}defer{emit(65+i)}for j in 0..2{if j==1{break}emit(48+j)}}return 0}
extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){emit(.code)}}fn main()->i32{var count=0;var callback=move()=>{if true{defer{count+=1}}return count};if callback()!=1{return 1}if callback()!=2{return 2}if callback()!=3{return 3}return 0}
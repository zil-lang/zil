extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){emit(.code)}}fn main()->i32{let token=Ticket{code:65};let read=()=>token.code;defer{emit(read())}let consumed=give token;return 0}
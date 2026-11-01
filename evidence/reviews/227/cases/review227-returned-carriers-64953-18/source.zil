extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){emit(.code)}}fn make_ticket()->Ticket contract{result(create)}{defer{emit(68)}return Ticket{code:65}}fn callback()->impl Fn()->i32{let token=Ticket{code:66};defer{emit(69)}return move()=>token.code}fn main()->i32{let token=make_ticket();let f=callback();if f()!=66{return 1}return 0}
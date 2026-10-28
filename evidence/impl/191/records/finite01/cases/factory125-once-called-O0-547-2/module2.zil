import helpers
import io
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){io.emit(.code)}}
fn consume(give ticket:Ticket){}
resource Model{count:i64}
impl Drop for Model{fn drop(self:Self){io.emit(82)}}
pub fn make(seed:i64)->impl FnOnce()->i64{let ticket=Ticket{code:73};let init=move (n:i64)=>{consume(give ticket);return seed+n};let prepared=helpers.once(init,2);let held=Ticket{code:82};return move ()=>{consume(give held);return prepared}}
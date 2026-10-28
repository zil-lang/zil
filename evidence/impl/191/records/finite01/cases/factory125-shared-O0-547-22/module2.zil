import helpers
import io
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){io.emit(.code)}}
fn consume(give ticket:Ticket){}
resource Model{count:i64}
impl Drop for Model{fn drop(self:Self){io.emit(82)}}
pub fn make(seed:i64)->impl Fn()->i64{let ticket=Ticket{code:73};let init=move (n:i64)=>{let observed=ticket.code;return n+2};let prepared=helpers.apply(&init,seed);let model=Model{count:prepared};return move ()=>model.count}
import helpers
import io
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){io.emit(.code)}}
fn consume(give ticket:Ticket){}
resource Model{count:i64}
impl Drop for Model{fn drop(self:Self){io.emit(82)}}
pub fn make(bad:bool)->impl Fn()->bool!i64{let ticket=Ticket{code:73};let init=move ()->bool!i64=>{consume(give ticket);if bad{return true}let answer:i64=42;return answer};let prepared=helpers.once_error(init);let held=Ticket{code:82};return move ()->bool!i64=>{let observed=held.code;return prepared}}
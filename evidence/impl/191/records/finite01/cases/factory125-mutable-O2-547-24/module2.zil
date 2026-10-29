import helpers
import io
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){io.emit(.code)}}
fn consume(give ticket:Ticket){}
resource Model{count:i64}
impl Drop for Model{fn drop(self:Self){io.emit(82)}}
pub fn make(seed:i64)->impl FnMut(i64)->i64{let ticket=Ticket{code:73};var count=seed;var init=move (n:i64)=>{let observed=ticket.code;count+=n;return count};let prepared=helpers.twice(&mut init);let model=Model{count:prepared};return move (delta:i64)=>{model.count+=delta;return model.count}}
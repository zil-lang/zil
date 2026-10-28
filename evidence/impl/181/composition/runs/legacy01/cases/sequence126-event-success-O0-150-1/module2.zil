import batch
import io
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){io.emit(.code)}}
pub fn run(stop:i64)->bool!i32{let ticket=Ticket{code:84};var total:i64=0;var event=(n:i64)->bool!i64=>{io.emit(69);if n==stop{return true}total+=n;return total};let result=batch.four(&mut event)?;if result!=20{return true}if total!=10{return true}io.emit(83);return 0}

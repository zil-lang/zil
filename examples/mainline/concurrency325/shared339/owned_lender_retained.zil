resource Ticket{value:i32}
impl Drop for Ticket{fn drop(self:Self){}}
fn make(value:i32)->impl Fn()->&i32{let ticket=Ticket{value:value};return move()=>{let kept=ticket.value;return &value}}
fn worker(input:&i32)->i32{return *input}
fn scoped(input:&i32)->i32{let result=thread_scope(worker,input,()->i32=>0);return result.1}
fn main()->i32{let owner=make(9);let input=owner();return scoped(input)-9}
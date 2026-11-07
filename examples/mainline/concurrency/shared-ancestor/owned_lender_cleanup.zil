resource Ticket{value:i32}
impl Drop for Ticket{fn drop(self:Self){}}
fn make(value:i32)->impl Fn()->&i32{let ticket=Ticket{value:value};return move()=>{let kept=ticket.value;return &value}}
fn worker(input:&i32)->i32{return *input}
fn main()->i32{let owner=make(9);let input=owner();let parent=move()->i32=>{let body=move()->i32=>{let taken=owner;return 0};let result=thread_scope(worker,input,body);return result.1};return parent()-9}
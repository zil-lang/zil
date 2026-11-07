resource Ticket{value:i32}
impl Drop for Ticket{fn drop(self:Self){}}
package Item{manages ticket:Ticket;value:i32}
fn worker(input:&i32)->i32{return *input}
fn scoped(input:&i32)->i32{
    let result=thread_scope(worker,input,()->i32=>0)
    return result.1
}
fn main()->i32{
    let owner=Item{ticket:Ticket{value:9},value:9}
    let input=&owner.value
    return scoped(input)-9
}

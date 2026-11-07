resource Ticket{value:i32}
impl Drop for Ticket{fn drop(self:Self){}}
package Item{manages ticket:Ticket;value:i32}
fn worker(input:&i32)->i32{return *input}
fn main()->i32{
    let owner=Item{ticket:Ticket{value:9},value:9}
    let input=&owner.value
    let parent=move()->i32=>{
        let body=move()->i32=>{let taken=owner;return 0}
        let result=thread_scope(worker,input,body)
        return result.1
    }
    return parent()-9
}

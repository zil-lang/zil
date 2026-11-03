resource Ticket{value:i32}
impl Drop for Ticket{fn drop(self:Self){}}
package Item{manages ticket:Ticket;value:i32}
fn worker(input:&i32)->i32{return *input}
fn scoped(input:&i32,give items:Store<Item>)->i32{
    let body=move()->i32=>{store_reserve(items,100);return 0}
    let result=thread_scope(worker,input,body)
    return result.1
}
fn main()->i32{
    let items:Store<Item>=store_new()
    store_reserve(items,1)
    store_push(items,Item{ticket:Ticket{value:9},value:9})
    let element=store_at(items,0)
    let input=&element.value
    return scoped(input,give items)-9
}

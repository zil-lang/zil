package Payload {manages bytes:ByteBuffer;pub count:i32}
fn worker(cell:Mutex<Payload>)->i32 {
    var guard=mutex_lock(cell)
    let value=mutex_value(&mut guard)
    value.count+=1
    return 0
}
fn main()->MutexCreateError!i32 {
    let bytes=bytes_new()
    let cell=mutex_create(Payload{bytes:give bytes,count:0})?
    let (created,child,parent)=thread_scope(worker,cell,()->i32=>worker(cell))
    if !created {return 1}
    return mutex_with(cell,(value:&mut Payload)->i32=>value.count-2)+child+parent
}

package Payload {manages bytes:ByteBuffer;pub count:i32}
fn other_thread(guard:MutexGuard<Payload>)->i32 {mutex_unlock(guard);return 0}
fn bad(cell:Mutex<Payload>)->i32 {
    let guard=mutex_lock(cell)
    let (created,child,parent)=thread_scope(other_thread,guard,()->i32=>0)
    if !created {return 1} return child+parent
}
fn main(){}

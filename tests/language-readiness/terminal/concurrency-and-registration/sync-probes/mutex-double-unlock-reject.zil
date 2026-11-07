package Payload {manages bytes:ByteBuffer;pub count:i32}
fn bad(cell:Mutex<Payload>)->i32 {
    let guard=mutex_lock(cell)
    let duplicate=guard
    mutex_unlock(guard)
    mutex_unlock(duplicate)
    return 0
}
fn main(){}

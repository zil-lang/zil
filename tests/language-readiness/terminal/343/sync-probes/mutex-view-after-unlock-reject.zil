package Payload {manages bytes:ByteBuffer;pub count:i32}
fn bad(cell:Mutex<Payload>)->i32 {
    var guard=mutex_lock(cell)
    let value=mutex_value(&mut guard)
    mutex_unlock(guard)
    return value.count
}
fn main(){}

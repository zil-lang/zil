package Payload {manages bytes:ByteBuffer;pub count:i32}
fn escape(cell:Mutex<Payload>)->&mut Payload {
    var guard=mutex_lock(cell)
    return mutex_value(&mut guard)
}
fn main(){}

package Payload {manages bytes:ByteBuffer;pub count:i32}
fn make(value:Payload)->MutexCreateError!Mutex<Payload> contract{result(create)} {
    return mutex_create(value)
}
fn main()->MutexCreateError!i32 {
    let bytes=bytes_new()
    let cell=make(Payload{bytes:give bytes,count:7})?
    var guard=mutex_lock(cell)
    let value=mutex_value(&mut guard)
    value.count+=1
    let answer=value.count
    mutex_unlock(guard)
    return answer-8
}

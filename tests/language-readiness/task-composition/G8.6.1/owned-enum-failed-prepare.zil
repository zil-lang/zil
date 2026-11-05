extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
enum Packet { Empty, Item(manages Token) }
fn worker(give token:Token)->i32 {
    let packet=Packet.Item(give token);
    task_yield();
    match packet {
        Packet.Empty()=>{return 0;},
        Packet.Item(value)=>{return value.code;}
    }
}
fn failed()->TaskCreateError!i32 { return TaskCreateError.Allocation; }
fn consume(packet:Packet,n:i32)->i32 { return n; }
fn main()->TaskCreateError!i32 {
    let input=Token{code:65};
    var task=task_create(worker,give input)?;
    task_poll(&mut task);
    let prepared=Token{code:66};
    // The first ordinary argument owns B; failure of the later argument first
    // cleans B, then default-close cancels the suspended Packet that owns A.
    return consume(Packet.Item(give prepared),failed()?);
}

package State<F>{manages bytes:ByteBuffer;a:F;b:F}
fn recycle<F:Fn()->i32>(state:State<F>)->State<F>{
    var next=state
    let view=&next.a
    let replacement=next.a
    next.a=replacement
    view()
    return next
}
fn main(){}

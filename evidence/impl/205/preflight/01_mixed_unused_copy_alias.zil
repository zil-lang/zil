package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}
fn replace<T:Copy,F:Fn()->i32>(state:State<T,F>,replacement:F)->State<T,F>{
    var next=state
    let view=&next.a
    next.a=replacement
    view()
    return next
}
fn main(){}

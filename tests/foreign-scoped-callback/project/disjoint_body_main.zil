import bridge
fn main()->i32{
    bridge.configure(0)
    var f=(value:i32)->i32=>{bridge.hit(70);return value+1}
    var observed=0
    let body=()->i32=>{
        bridge.body()
        for event in 2..4{
            if bridge.pump(event)!=event+1{return 122}
            observed+=1
        }
        return -17
    }
    let result=bridge.run(&mut f,body)
    if !result.0||result.1!= -17||observed!=2||bridge.roots()!=0{return 123}
    if f(4)!=5{return 124}
    return 0
}

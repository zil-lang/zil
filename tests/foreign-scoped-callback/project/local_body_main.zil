import local_body_bridge
fn main()->i32{
    local_body_bridge.configure(0)
    var count=0
    var f=move(value:i32)->i32=>{local_body_bridge.hit(76);count+=1;return value+count}
    let result=local_body_bridge.run(&mut f)
    if !result.0||result.1!=3||local_body_bridge.roots()!=0{return 123}
    if f(4)!=8{return 124}
    return 0
}

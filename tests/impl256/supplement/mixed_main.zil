import model
import relay
fn mixed(right:bool)->i32{
    model.emit(83)
    defer{model.emit(81)}
    let first=model.marker(65)
    let second=model.marker(66)
    var selected=if right{second}else{first}
    let current=relay.identity(selected)
    let result=current(1)
    return result-(if right{67}else{66})
}
fn main()->i32{
    let mode=model.read()-48
    if mode==0{return mixed(false)}
    if mode==1{return mixed(true)}
    return 9
}

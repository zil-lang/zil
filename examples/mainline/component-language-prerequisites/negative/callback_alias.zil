import component
import bridge
fn main()->i32 {
    let mark=component.Mark{code:72}
    var handler=move(value:i32)->i32=>value+mark.code
    let alias=&handler
    let body=()->i32=>0
    let result=bridge.run(&mut handler,body)
    return alias(1)+result.1
}

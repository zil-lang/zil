import rules;
import component;
fn main()->i32{
    let value=component.Component{};
    let handler=rules.handler(&value);
    if handler(5000000000,7)!=5000000007{return 1}
    return 0
}

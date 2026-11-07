import model
import state
fn main()->model.Fault!i32 {
    var s=state.relay(state.make(false)?)
    var items=s.items
    let old=items[0]
    items[0]=model.make(67,false)?
    s.items=items
    s.items=[model.make(68,false)?,model.make(69,false)?]
    return old.code-65
}

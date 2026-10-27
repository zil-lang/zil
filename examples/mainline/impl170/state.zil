import model
pub package State { manages tag:model.Ticket; pub items:[model.Item;2] }
pub fn make(fail:bool)->model.Fault!State {
    return State { tag:model.Ticket{code:90},items:[model.make(65,false)?,model.make(66,fail)?] }
}
pub fn relay(s:State)->State { return s }
pub fn error()->State!i32 { return State{tag:model.Ticket{code:90},items:[model.lossy(65),model.lossy(66)]} }

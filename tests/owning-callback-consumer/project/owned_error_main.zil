import model
import dispatch
fn main()->model.OwnedFault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let active=model.Mark{code:72}
    let spare=model.Mark{code:104}
    let handler=move(event:model.Event)->model.OwnedFault!model.Reply=>{
        model.emit(67)
        let held=give active
        let seen=spare.code
        return model.OwnedFault{packet:model.Packet.Data(event)}
    }
    let reply=dispatch.owned_error(handler,model.prepare_owned_error()?)?
    return model.render(reply,42)
}

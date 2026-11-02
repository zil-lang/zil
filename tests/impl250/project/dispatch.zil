import model
pub fn dispatch<F:FnOnce(model.Event)->model.Reply>(handler:F,event:model.Event)->model.Reply{
    return handler(event)
}
pub fn packet<F:FnOnce(model.Packet)->model.Packet>(handler:F,event:model.Packet)->model.Packet{
    return handler(event)
}
pub fn fallible<F:FnOnce(model.Fault!model.Packet)->model.Fault!model.Packet>(handler:F,event:model.Fault!model.Packet)->model.Fault!model.Packet{
    return handler(event)
}
pub fn items<F:FnOnce(model.Items)->model.Items>(handler:F,event:model.Items)->model.Items{
    return handler(event)
}

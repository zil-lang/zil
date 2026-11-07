import model
import transport

pub fn run() -> model.Fault!i32 {
    var slots = transport.relay([model.make(65, false)?, model.make(66, false)?])
    let old = slots[0]
    slots[0] = model.make(67, false)?
    return old.code + slots[0].code + slots[1].code - 198
}

pub fn prepare_failure() -> model.Fault!i32 {
    let slots = [model.make(65, false)?, model.make(66, true)?]
    return slots[0].code
}

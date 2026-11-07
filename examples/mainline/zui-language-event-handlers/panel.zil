import model

pub package Panel { manages tag: model.Ticket; pub children: [model.Item; 2]; pub revision: i32 }
pub fn relay(panel: Panel) -> Panel { return panel }
pub fn make() -> Panel {
    return Panel { tag: model.Ticket { code: 90 }, children: [model.item(65), model.item(66)], revision: 1 }
}
pub fn prepare() -> model.Fault!Panel {
    return Panel { tag: model.Ticket { code: 90 }, children: [model.prepare(65, false)?, model.prepare(66, false)?], revision: 1 }
}

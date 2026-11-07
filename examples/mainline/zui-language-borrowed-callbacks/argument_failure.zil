import model

fn consume(value: model.Fault!model.Item, later: i32) -> model.Fault!i32 {
    let item = value?
    return item.code + later
}

fn fail() -> model.Fault!i32 {
    let item = model.make(66, true)?
    return item.code
}

fn main() -> model.Fault!i32 { return consume(model.make(65, false), fail()?)? }

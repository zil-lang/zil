import model
import arrays
import callbacks

fn result_roundtrip() -> model.Fault!i32 {
    let item = model.relay(model.relay(model.make(65, false)))?
    return item.code - 65
}

fn main() -> model.Fault!i32 {
    if result_roundtrip()? != 0 { return 1 }
    if arrays.run()? != 0 { return 2 }
    if callbacks.run() != 0 { return 3 }
    return 0
}

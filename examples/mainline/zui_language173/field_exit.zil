import model
import panel

fn main() -> model.Fault!i32 {
    let state = panel.prepare()?
    let children = state.children
    return model.Fault.Prepare
}

import model
import panel

fn main() -> model.Fault!i32 {
    var state = panel.prepare()?
    state.children = [model.prepare(67, false)?, model.prepare(68, true)?]
    return 0
}

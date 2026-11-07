import state

fn main() -> void!i32 {
    return state.take_then_error(state.make()?)
}

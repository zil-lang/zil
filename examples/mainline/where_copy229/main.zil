import helper
import model

fn main() -> i32 {
    let pair = helper.duplicate(model.Cell { value: 7 })
    return pair.0.value + pair.1.value - 14
}

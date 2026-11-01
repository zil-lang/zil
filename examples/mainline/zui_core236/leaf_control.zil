// Three-module existing-shape control; it does not exercise 227 or 235.
import model
import portable
fn main() -> i32 {
    let value = model.node(0)
    if !value.ready { return 1 }
    return 0
}

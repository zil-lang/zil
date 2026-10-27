import model
import state
import callbacks
extern "C" fn getchar() -> i32;

fn main() -> i32 {
    var input = 0
    unsafe { input = getchar() }
    if input < 48 || input > 51 { return 9 }
    let controller = model.Controller { count: input - 48 }
    let count = controller.read()
    var current = state.create(callbacks.make(72), count)
    if !current.ready { return 1 }
    let expected = count * 68 + count * (count - 1) / 2
    if current.sum != expected { return 2 }
    current = state.tick(current)
    if current.hits != 1 { return 3 }
    current = state.tick(current)
    if current.hits != 2 { return 4 }
    current = state.replace_children(current, count + 1)
    if !current.ready {
        current = state.tick(current)
        if current.hits != 3 || current.sum != expected { return 5 }
        return 1
    }
    if current.sum != (count + 1) * 77 + count * (count + 1) / 2 { return 6 }
    current.callback = callbacks.make(75)
    current = state.tick(current)
    if current.hits != 1 { return 7 }
    current = state.tick(current)
    if current.hits != 2 { return 8 }
    current = state.grow(current, i64(count + 2))
    if !current.ready { return 10 }
    return 0
}

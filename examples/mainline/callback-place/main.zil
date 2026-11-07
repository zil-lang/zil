import factory
import model
import state
extern "C" fn getchar() -> i32;

fn main() -> i32 {
    var input = 0
    unsafe { input = getchar() }
    if input < 48 || input > 55 { return 9 }
    model.emit(65)
    return state.drive(state.pack(factory.make(), model.Counter { code: 65 }), input - 48)
}

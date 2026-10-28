import helpers
import io

resource Initializer { code: i32 }
impl Drop for Initializer {
    fn drop(self: Self) { io.emit(.code); }
}

resource Model { count: i64 }
impl Drop for Model {
    fn drop(self: Self) { io.emit(82); }
}

pub fn make(seed: i64) -> impl FnMut(i64) -> i64 {
    let owner = Initializer { code: 73 };
    let init = move (n: i64) => {
        let observed = owner.code;
        return n + 2;
    };
    let prepared = helpers.initialize(&init, seed);
    let model = Model { count: prepared };
    var handler = move (delta: i64) => {
        model.count += delta;
        return model.count;
    };
    let warmed = helpers.warm(&mut handler);
    return handler;
}

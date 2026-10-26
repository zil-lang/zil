import api
import model
import io

resource Model { count: i64 }
impl Drop for Model {
    fn drop(self: Self) { io.emit(82); }
}

pub fn make(seed: i64) -> impl FnMut(api.Event) -> i64 {
    let init = (event: api.Event) =>
        model.unpack(event) + #(i64(2) << i64(1));
    let prepared = api.initialize(&init, model.pack(seed));
    let state = Model { count: prepared };
    var handler = move (event: api.Event) => {
        state.count += model.unpack(event);
        return state.count >> i64(1);
    };
    let warmed = api.warm(&mut handler);
    return handler;
}

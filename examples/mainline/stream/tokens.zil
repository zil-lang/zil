import io
import model

struct State {
    value: i32
    negative: bool
    seen: bool
    has_digit: bool
    error: i32
}

fn blank(value: i32) -> bool => value == 32 || (value >= 9 && value <= 13)

fn push(state: &mut State, byte: i32) {
    if !state.seen && (byte == 45 || byte == 43) {
        state.seen = true
        state.negative = byte == 45
        return
    }
    state.seen = true
    if byte < 48 || byte > 57 {
        if state.error == 0 { state.error = 1 }
        return
    }
    state.has_digit = true
    if state.error != 0 { return }
    let digit = byte - 48
    if state.value > 100000 || (state.value == 100000 && digit > 0) {
        state.error = 2
        return
    }
    state.value = state.value * 10 + digit
}

fn finish(state: &State) -> model.Result<i32, i32> {
    if state.error != 0 { return model.Result.Err(state.error) }
    if !state.has_digit { return model.Result.Err(1) }
    if state.negative { return model.Result.Ok(-state.value) }
    return model.Result.Ok(state.value)
}

pub fn next() -> model.Option<model.Result<i32, i32>> {
    var state = State { value: 0, negative: false, seen: false, has_digit: false, error: 0 }
    while true {
        let byte = io.read()
        if byte < 0 {
            if state.seen { return model.Option.Some(finish(&state)) }
            return model.Option.None
        }
        if blank(byte) {
            if state.seen { return model.Option.Some(finish(&state)) }
            continue
        }
        push(&mut state, byte)
    }
}

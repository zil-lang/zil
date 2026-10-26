import io
import token

fn described() -> i32 {
    let r = token.make(65)
    let read = () => token.read(r)
    if read() != 65 { return 1 }
    token.bump(r, 1)
    if read() != 66 { return 2 }
    let write = () => { token.bump(r, 1); }
    write()
    if read() != 67 { return 3 }
    return 0
}

fn owned_shared() -> i32 {
    let r = token.make(65)
    let read = move () => token.read(r)
    if read() != 65 { return 1 }
    if read() != 65 { return 2 }
    return 0
}

fn uncalled() -> i32 {
    let r = token.make(65)
    let s = token.make(66)
    let unused = move () -> i32 => { token.read(r); token.read(s) }
    return 0
}

fn consumed() -> i32 {
    let r = token.make(65)
    let s = token.make(66)
    let once = move () => {
        token.read(s)
        token.consume(give r, 2)
        9
    }
    if once() != 9 { return 1 }
    return 0
}

fn owned_mutable() -> i32 {
    let r = token.make(65)
    var update = move () => { token.bump(r, 1); token.read(r) }
    if update() != 66 { return 1 }
    if update() != 67 { return 2 }
    return 0
}

fn early() -> i32 {
    let r = token.make(65)
    let unused = move () -> i32 => token.read(r)
    return 7
}

fn transferred() -> i32 {
    let r = token.make(65)
    let first = move () => token.read(r)
    let second = first
    if second() != 65 { return 1 }
    return 0
}

fn loop_exit() -> i32 {
    for i in 0..3 {
        let r = token.make(65 + i)
        let read = move () => token.read(r)
        if i == 0 { continue }
        if read() != 66 { return 1 }
        break
    }
    return 0
}

// Explicit finite error channel; E!T and postfix propagation are a later slice.
enum Outcome { Ok(i32), Failed(i32) }
fn transaction(stage: i32) -> Outcome {
    let outer = token.make(65)
    if stage == 0 { return Outcome.Failed(1) }
    let item = token.make(66)
    let callback = move () => token.read(item)
    if stage == 1 { return Outcome.Failed(2) }
    let tail = token.make(67)
    if stage == 2 { return Outcome.Failed(3) }
    if callback() != 66 { return Outcome.Failed(4) }
    return Outcome.Ok(0)
}

fn main() -> i32 {
    let command = io.read()
    if command >= 48 && command <= 51 {
        let result = transaction(command - 48)
        match result {
            Outcome.Ok(value) => { return value }
            Outcome.Failed(error) => { return error }
        }
    }
    if command == 68 { return described() }
    if command == 83 { return owned_shared() }
    if command == 85 { return uncalled() }
    if command == 79 { return consumed() }
    if command == 77 { return owned_mutable() }
    if command == 69 { return early() }
    if command == 84 { return transferred() }
    if command == 76 { return loop_exit() }
    return 8
}

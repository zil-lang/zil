import io
import token
import failure

fn transaction(stage: i32) -> failure.Fault!i32 {
    let outer = token.make(65)
    failure.check(stage, 0)?
    let item = token.make(66)
    let callback = move () => token.read(item)
    failure.check(stage, 1)?
    let tail = token.make(67)
    failure.check(stage, 2)?
    if callback() != 66 { return failure.Fault.Stop(8) }
    return 7
}

fn pending_argument(fail: bool) -> failure.Fault!i32 {
    let outer = token.make(65)
    let argument = token.make(66)
    let answer = token.accept(give argument, failure.value(fail)?)
    return answer - 66
}

fn pending_callee(fail: bool) -> failure.Fault!i32 {
    let outer = token.make(65)
    let item = token.make(66)
    let once = move (amount: i32) => token.accept(give item, amount)
    let answer = once(failure.value(fail)?)
    return answer - 66
}

fn partial_constructor(fail: bool) -> failure.Fault!i32 {
    let outer = token.make(65)
    let item = token.Token { code: 66 + failure.value(fail)? }
    return token.read(item) - 66
}

fn closure_error(fail: bool) -> failure.Fault!i32 {
    let outer = token.make(65)
    let item = token.make(66)
    let once = move () -> failure.Fault!i32 => {
        if fail { failure.check(0, 0)? }
        token.accept(give item, 0)
        0
    }
    return once()
}

fn loop_error(stage: i32) -> failure.Fault!i32 {
    let outer = token.make(65)
    for i in 0..3 {
        let item = token.make(66 + i)
        let callback = move () => token.read(item)
        if i == 0 { continue }
        failure.check(i, stage)?
        if callback() == 68 { break }
    }
    return 0
}

fn main() -> failure.Fault!i32 {
    let command = io.read()
    if command >= 48 && command <= 51 { return transaction(command - 48) }
    if command == 65 { return pending_argument(true) }
    if command == 97 { return pending_argument(false) }
    if command == 70 { return pending_callee(true) }
    if command == 102 { return pending_callee(false) }
    if command == 80 { return partial_constructor(true) }
    if command == 112 { return partial_constructor(false) }
    if command == 67 { return closure_error(true) }
    if command == 99 { return closure_error(false) }
    if command == 76 { return loop_error(1) }
    if command == 108 { return loop_error(9) }
    return failure.Fault.Stop(9)
}

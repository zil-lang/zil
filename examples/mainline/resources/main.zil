import io
import token

fn main() -> i32 {
    let command = io.read()
    let outer = token.make(65)
    if true {
        let x = token.make(66)
        let alias = x
        token.bump(alias, 1)
        let y = token.relay(give x)
        let result = token.consume(token.make(68), 1)
        if result != 69 { return 3 }
        token.read(token.make(73))
        if command == 33 { return 2 }
        if token.read(y) != 67 { return 4 }
    }
    for i in 0..4 {
        let item = token.make(70 + i)
        if i == 0 { continue }
        if i == 1 { break }
    }
    let tail = token.make(72)
    io.emit(124)
    return 0
}

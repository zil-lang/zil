enum Step { End, Item(i64, bool) }

fn next(index:i64)->Step {
    if index == 8 { return Step.End }
    return Step.Item(index + 1, index % 2 == 0)
}

fn result()->i64 {
    var index:i64=0
    var total:i64=0
    while let Step.Item(value, keep) = next(index) {
        index += 1
        if let false = keep { continue }
        total += value
        if let 7 = value { break }
    }
    if let Step.End() = next(8) {
        return total * 100 + index
    }
    return 0
}

fn main()->i32 {
    if result() == 1607 { return 0 }
    return 1
}

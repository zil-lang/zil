fn choose(flag:bool)->i32|bool {
    if flag { return 40 }
    return false
}
fn score(value:bool|i32)->i32 {
    match value {
        i32(number) => { return number }
        bool(flag) => { if flag { return 1 } return 2 }
    }
}
fn result()->i32 { return score(choose(true)) + score(choose(false)) }

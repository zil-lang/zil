// 32-bit target: success. 64-bit target: bool type selection rejects integer binding.
fn select()->Type{
    let high=0usize-%1usize;
    if high>4294967295usize{return bool}
    return usize
}
fn result()->i32{let value:#(select())=7;return 0}

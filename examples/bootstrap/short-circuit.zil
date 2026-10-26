fn main() -> i32 {
    let safe = false && (1 / 0 == 1)
    let also_safe = true || (1 / 0 == 1)
    return if safe { 0 } else { if also_safe { 42 } else { 1 } }
}

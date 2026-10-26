enum Failure { Bad }
fn bad(parse: fn(i32) -> Failure!i32, next: fn(i32) -> i32) -> i32 {
    return 1 |> parse |> next
}

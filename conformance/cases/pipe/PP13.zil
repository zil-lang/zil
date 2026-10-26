enum Failure { Bad }
fn apply(parse: fn(i32) -> Failure!i32) -> Failure!i32 {
    return (1 |> parse)?
}

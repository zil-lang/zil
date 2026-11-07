// The body N is a runtime local. It cannot inherit the declaration's const
// length identity, even if no concrete instance of this function is requested.
fn invalid<#N: i32>(values: [i32; N]) -> [i32; N] {
    let N = 3
    let result: [i32; N] = values
    return result
}
fn main() {}

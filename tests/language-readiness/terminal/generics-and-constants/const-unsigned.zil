fn answer<#N: u64>() -> u64 { return N }
fn relay<#M: u64>() -> u64 { return answer::<M>() }
fn keep<#N: usize>(values: [i32; N]) -> [i32; N] { return values }
fn literal<T: Copy>(value: T) -> u64 { return 18446744073709551615u64 }
fn main() -> i32 {
    let values = keep::<3usize>([2, 3, 7]);
    let high = relay::<18446744073709551615u64>();
    if high == literal(true) { return values[2] - 7 }
    return 1
}

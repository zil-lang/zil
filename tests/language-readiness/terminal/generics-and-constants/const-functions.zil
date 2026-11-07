fn fixed<T: Copy, #N: i32>(values: [T; N]) -> [T; N] {
    return values
}

fn scalar<#N: i32>() -> i32 {
    return N
}

fn forwarded<#M: i32>() -> i32 {
    return scalar::<M>()
}

fn result() -> i32 {
    let a = fixed::<i32, 2>([7, 9])
    let b = fixed::<i32, 1>([a[0]])
    return b[0] + forwarded::<3>()
}

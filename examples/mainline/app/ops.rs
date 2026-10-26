pub fn double(value: i64) -> i64 {
    value * 2
}

pub fn offset(value: i64) -> i64 {
    value + 11
}

pub fn at(values: &[i64; 8], index: i64) -> &i64 {
    &values[index as usize]
}

pub fn relay(value: &i64) -> &i64 {
    value
}

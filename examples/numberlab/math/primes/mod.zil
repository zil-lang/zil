fn is_prime(value: i32) -> bool {
    if value < 2 {
        return false
    }
    var divisor = 2
    while divisor <= value / divisor {
        if value % divisor == 0 {
            return false
        }
        divisor = divisor + 1
    }
    return true
}

pub fn count_below(limit: i32) -> i32 {
    var count = 0
    for value in 2..limit {
        if is_prime(value) {
            count = count + 1
        }
    }
    return count
}

pub fn sum_below(limit: i32) -> i32 {
    var sum = 0
    for value in 2..limit {
        if is_prime(value) {
            sum = sum + value
        }
    }
    return sum
}

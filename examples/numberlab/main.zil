import math.gcd as euclid
import math.primes

// The calculation is performed by the compiled ZIL functions.
pub fn result() -> i32 {
    return euclid.gcd(252, 105) + primes.count_below(1000) + primes.sum_below(1000)
}

fn main() -> i32 {
    if result() == 76316 && euclid.gcd(0, 0) == 0 && primes.count_below(2) == 0 {
        return 0
    }
    return 1
}

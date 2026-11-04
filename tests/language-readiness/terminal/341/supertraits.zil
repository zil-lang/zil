trait Base {
    fn base(self: &Self) -> i32;
}

trait Child: Base {
    fn child(self: &Self) -> i32 { return self.base() }
}

struct Point { n: i32 }

// A child recipe is still provisional until the Base premise is available.
impl Child for Point {}
impl Base for Point {
    fn base() -> i32 { return .n }
}

fn consume<T: Child>(point: &T) -> i32 { return point.child() }

fn main() -> i32 {
    let point = Point { n: 7 }
    return consume(&point) - 7
}

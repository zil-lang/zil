fn suspended() -> i32 contract { effects(suspend); } { return 7 }
fn main() -> i32 { return 0 }

import generators as gen
fn main()->i32 { return #(gen.plus_one(quote<Expr>{1})) }

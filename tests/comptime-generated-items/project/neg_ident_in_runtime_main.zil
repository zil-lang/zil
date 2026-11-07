import std.syntax as syntax
extern "C" fn leak(value:Ident)->i32;
fn main()->i32 { unsafe { return leak(#(syntax.fresh("local"))) } }

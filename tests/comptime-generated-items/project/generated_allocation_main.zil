import std.syntax as syntax
import generators as gen

#emit gen.fallible(syntax.export("make_generated"),syntax.export("check_generated"))
fn main()->void!i32 {
    let item=make_generated(false)?;
    return check_generated(item)
}

import std.syntax as syntax
import generators as gen
#emit gen.copy_record(syntax.export("Left"))
#emit gen.copy_record(syntax.export("Right"))
fn main()->i32 {
    let left=Left{value:7};
    let right:Right=left;
    return right.value
}

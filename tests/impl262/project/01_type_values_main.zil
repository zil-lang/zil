import std.meta as meta
import std.syntax as syntax
import common.model as model
import generators as gen

type Four=#(gen.four(gen.type_id(i32)))
#emit gen.getter(model.Point,syntax.export("get_x"))
fn main()->i32 {
    let scalar:#(i32)=1;
    let values:Four=[scalar,2,3,4];
    let point=model.Point{x:5,y:8};
    if values[3]!=4 || get_x(point)!=5 { return 1 }
    if #(gen.size_of(model.Point))!=8 { return 2 }
    if #(gen.align_of(model.Point))!=4 { return 3 }
    return 0
}

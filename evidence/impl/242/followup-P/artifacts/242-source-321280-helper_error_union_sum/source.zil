
resource R{n:i32}
enum E{Empty,Pair(manages R,manages R)}
package Wrap{state:E;ready:bool}
fn sink(give owner:R){}
fn make()->E{return E.Pair(R{n:65},R{n:66})}
fn fallible()->void!Wrap{return Wrap{state:make(),ready:true}}fn use_value<F:Fn()->i32>(callback:F,value:void!Wrap)->i32{return callback()}fn main()->i32{return use_value(()=>7,fallible())-7}
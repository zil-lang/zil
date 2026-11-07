enum Inner { Pair(i32,i32), None }
enum Outer { Nested(Inner), None }
fn result()->i32 {
  var value=Outer.Nested(Inner.Pair(20,22));
  match value {
    Outer.Nested(Inner.Pair(ref mut left,ref mut right))=>{
      *left+=*right;
      return *left
    },
    Outer.Nested(Inner.None())=>{return 0},
    Outer.None()=>{return 0}
  }
}

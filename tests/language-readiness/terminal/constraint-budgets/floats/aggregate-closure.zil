struct Pack{value:f64,items:[f32;2]}
enum Choice{Number(f32),Empty}
fn single(x:f32)->f32{return x+1.0f32}
fn same<T:Copy>(value:T)->T{return value}
fn make()->Pack{return Pack{value:3.5,items:[1.0f32,2.0f32]}}
fn result()->i32{
    let raw=same(1.0f32);let mixed=same(1.0+2.0f32);
    let pack=#(make());
    let choice=Choice.Number(1.0+2.0f32);
    let pointer:fn(f32)->f32=single;
    var state:f32=1.0;
    var step=()=>{state=state+0.5f32;return state};
    let a=step();let b=step();
    let wrapped=same(pack);
    match choice{Choice.Number(n)=>{if raw==1.0f32 && mixed==3.0f32 && n==3.0f32 && pointer(n)==4.0f32 && a==1.5f32 && b==2.0f32 && wrapped.value==3.5 && wrapped.items[1]==2.0f32{return 344}return 0},Choice.Empty()=>{return 0}}
}

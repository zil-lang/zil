import a;
import b;
fn select(which:bool)->DynCreateError!OwnedDyn<dyn a.Read>{
    if which{return a.first()}
    return b.second()
}
fn read_one(which:bool)->DynCreateError!i32{
    let owned=select(which)?;
    return a.read_owned(owned)
}
fn main()->i32{
    let left=read_one(true);
    let right=read_one(false);
    var total:i32=0;
    match left{.ok(v)=>{total+=v},.err(_)=>{return 100}}
    match right{.ok(v)=>{total+=v},.err(_)=>{return 100}}
    return total
}

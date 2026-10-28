import model;
import operations;
import visits;
extern "C" fn getchar()->i32;

fn main()->i32{
    var input=0;
    unsafe{input=getchar()};
    let count=input-48;
    if count<0||count>3{return 9};
    let anchor=model.make(65,10);
    let counter=model.make(66,0);
    let read=anchor;
    let edit=counter;
    let head=&anchor;
    let field=&head.n;
    let callback=()=>operations.read(read)+*field+operations.bump(edit);
    var total=0;
    for i in 0..count{total+=visits.step(&callback)};
    operations.set(anchor,20);
    return total-(20*count+count*(count+1)/2)
}

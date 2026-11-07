import model;
import operations;
import visits;
extern "C" fn getchar()->i32;

fn main()->bool!i32{
    var input=0;
    unsafe{input=getchar()};
    let count=input-48;
    if count<0||count>3{return 9};
    let outer=model.mark(90);
    let source=operations.make();
    if !bytes_reserve(source,1){return true};
    if !bytes_push(source,u8(65)){return true};
    let source_guard=model.mark(83);
    let target=operations.make();
    let target_guard=model.mark(84);
    let read=source;
    let edit=target;
    let view=bytes_view(source);
    let callback=()->bool!i32=>{
        let attempt=model.mark(70);
        return operations.read(read)+operations.append(edit)?
    };
    var total=0;
    for i in 0..count{total+=visits.step(&callback)?};
    let anchor=i32(view[0]);
    if !operations.clear(source)||!operations.clear(target){return true};
    operations.consume(give target);
    return total-(count+count*(count+1)/2)+anchor-65
}

import model;
import collection;
import adapter;
extern "C" fn getchar()->i32;

fn main()->model.Error!i32 {
    var input=0;unsafe { input=getchar() };
    let mode=input-48;
    if mode<0 || mode>4 { return 9 };
    let guard=model.marker(77);
    let pending=adapter.accept(collection.checked(model.item(65),model.item(66),mode==1));
    if mode==4 { return 0 };
    let first=pending?;
    let second=adapter.forward(collection.checked(model.other(67),model.other(68),mode==2))?;
    if mode==3 { return model.Error.Logical };
    if !model.score_first(first) || !model.score_second(second) { return model.Error.Logical };
    model.emit(90);
    return 0
}

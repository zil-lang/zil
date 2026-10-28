import model;
import factory;
import adapter;
extern "C" fn getchar()->i32;

fn main()->model.Error!i32 {
    var command:i32=0;
    unsafe { command=getchar() };
    let mode=command-48;
    if mode<0 || mode>5 { return 9 };
    let marker=adapter.marker(factory.marker(mode))?;
    if mode==4 {
        adapter.buffer(factory.buffer(0));
        adapter.store(factory.store(0));
        return 0
    };
    let buffer=adapter.buffer(factory.buffer(mode))?;
    let store=adapter.store(factory.store(mode))?;
    if mode==3 { return model.Error.Failed };
    let bytes=bytes_view(buffer);
    model.emit(i32(bytes[0]));
    return i32(bytes[0])+model.score(store)-72
}

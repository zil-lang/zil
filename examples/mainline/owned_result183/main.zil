import model;
import factory;
import adapter;
extern "C" fn getchar()->i32;
extern "C" fn putchar(code:i32)->i32;

fn main()->model.Error!i32 {
    let mode=unsafe { getchar() }-48;
    if mode<0 || mode>4 { return 9 };
    let marker=model.marker(77);
    if mode==4 {
        adapter.buffer(factory.buffer(0));
        adapter.store(factory.store(0));
        return 0
    };
    let buffer=adapter.buffer(factory.buffer(mode))?;
    let store=adapter.store(factory.store(mode))?;
    if mode==3 { return model.Error.Failed };
    let bytes=bytes_view(buffer);
    unsafe { putchar(i32(bytes[0])) };
    return i32(bytes[0])+model.score(store)-72
}

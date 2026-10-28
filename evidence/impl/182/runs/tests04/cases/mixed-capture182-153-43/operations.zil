import model;
extern "C" fn putchar(x:i32)->i32;

fn update(counter:model.Counter)->i32{
    counter.n+=1;
    unsafe{putchar(48+counter.n)};
    return counter.n
}
pub fn bump(counter:model.Counter)->i32{
    let callback=()=>update(counter);
    return callback()
}
pub fn read(counter:model.Counter)->i32{
    let callback=()=>counter.n;
    return callback()
}
pub fn set(counter:model.Counter,n:i32){counter.n=n}

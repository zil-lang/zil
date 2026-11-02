extern "C" fn getchar()->i32;
extern "C" fn putchar(code:i32)->i32;
fn read()->i32{unsafe{return getchar()}}
fn emit(code:i32){unsafe{putchar(code)}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
package Leaf{manages mark:Mark;manages bytes:ByteBuffer}
fn make_flat(fail:bool)->void!Leaf{
    let mark=Mark{code:65}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,1){return ()}
    if fail{return ()}
    bytes_push(bytes,7u8)
    return Leaf{mark:give mark,bytes:give bytes}
}
package SubscriptionSet{manages items:Store<Leaf>}
package Session{subscriptions:SubscriptionSet;ready:bool}
fn session()->Session{
    let items:Store<Leaf>=store_new()
    if store_reserve(items,1){
        let bytes=bytes_new()
        store_push(items,Leaf{mark:Mark{code:65},bytes:give bytes})
    }
    return Session{subscriptions:SubscriptionSet{items:give items},ready:true}
}
enum MaybeSession{Empty,Some(Session)}
fn main()->i32{
    let input=read()
    emit(83)
    let value=if input==49{MaybeSession.Empty}else{MaybeSession.Some(session())}
    emit(82)
    return 0
}

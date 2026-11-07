package P{manages bytes:ByteBuffer;n:i32}
fn empty()->Store<P> contract{result(create)}{return store_new()}
fn read(p:&i32)->i32{return *p}
fn main()->i32{let s=empty();store_reserve(s,1);store_push(s,P{bytes:bytes_new(),n:7});let item=store_at(s,0);let p=&item.n;let outer=move()->i32=>{let body=move()->i32=>{store_reserve(s,4);return 0};let(created,child,parent)=thread_scope(read,p,body);return 0};return outer()}

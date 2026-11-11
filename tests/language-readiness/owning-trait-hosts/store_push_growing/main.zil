import std.store
extern "C" fn putchar(value:i32)->i32;
resource Token { value:i32 }
impl Drop for Token {fn drop(self:Self){unsafe{putchar(.value)}}}
package Item {manages token:Token;pub id:i32}

fn item(expected:i32,created:&mut i32)->Item {
    *created+=1
    unsafe{putchar(expected+32)}
    return Item{token:Token{value:expected},id:expected}
}
fn prefix(items:Store<Item>,length:i64)->bool {
    if store.len(items)!=length {return false}
    var at:i64=0
    while at<length {
        if store_at(items,at).id!=65+i32(at) {return false}
        at+=1
    }
    return true
}
fn push_owned(items:Store<Item>,expected:i32,created:&mut i32)->i32 {
    let length=store.len(items)
    let capacity=store.capacity(items)
    let count=*created
    match store.push_growing(items,item(expected,created)) {
        .err(returned)=>{
            if returned.id!=expected || *created!=count+1 {return 11}
            if store.capacity(items)!=capacity || !prefix(items,length) {return 12}
            match store.push_growing(items,returned) {
                .err(_)=>{return 13},
                .ok(_)=>{}
            }
        },
        .ok(_)=>{}
    }
    if *created!=count+1 || !prefix(items,length+1) {return 14}
    if store.capacity(items)<length+1 {return 15}
    return 0
}
fn push_copy(items:Store<i64>,expected:i64)->i32 {
    let length=store.len(items)
    let capacity=store.capacity(items)
    match store.push_growing(items,expected) {
        .err(returned)=>{
            if returned!=expected || store.len(items)!=length || store.capacity(items)!=capacity {return 21}
            if length>0 && *store_at(items,0)!=10 {return 22}
            match store.push_growing(items,returned) {
                .err(_)=>{return 23},
                .ok(_)=>{}
            }
        },
        .ok(_)=>{}
    }
    if store.len(items)!=length+1 || *store_at(items,length)!=expected {return 24}
    if length>0 && *store_at(items,0)!=10 {return 25}
    return 0
}
fn dispose(value:Item){}
fn main()->i32 {
    let items:Store<Item>=store.new()
    var created:i32=0
    let first=push_owned(items,65,&mut created)
    if first!=0 {return first}
    let second=push_owned(items,66,&mut created)
    if second!=0 {return second}
    let third=push_owned(items,67,&mut created)
    if third!=0 {return third}
    if created!=3 || !prefix(items,3) || store.capacity(items)!=4 {return 31}
    let fourth=push_owned(items,68,&mut created)
    if fourth!=0 {return fourth}
    if created!=4 || !prefix(items,4) || store.capacity(items)!=4 {return 36}
    let values:Store<i64>=store.new()
    let fifth=push_copy(values,10)
    if fifth!=0 {return fifth}
    let sixth=push_copy(values,20)
    if sixth!=0 {return sixth}
    if store.len(values)!=2 || store.capacity(values)!=2 {return 32}
    var expected:i32=68
    while store.len(items)>0 {
        match store.pop(items) {
            .err(_)=>{return 33},
            .ok(value)=>{
                if value.id!=expected {return 34}
                dispose(value)
                expected-=1
            }
        }
    }
    if expected!=64 || store.len(items)!=0 || created!=4 {return 35}
    return 0
}

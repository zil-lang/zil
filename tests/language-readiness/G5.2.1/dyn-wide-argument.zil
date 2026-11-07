trait Read{fn read(self:&Self,marker:u64)->u64;}
struct A{bits:u64}
struct B{bits:u64}
impl Read for A{fn read(self:&Self,marker:u64)->u64{return self.bits}}
impl Read for B{fn read(self:&Self,marker:u64)->u64{return self.bits}}
fn read_it(x:&dyn Read,marker:u64)->u64{return x.read(marker)}
fn select(which:bool,marker:u64)->u64{
    let a=A{bits:18446744073709551615u64};
    let b=B{bits:42u64};
    if which{return read_it(&a,marker)}else{return read_it(&b,marker)}
}
extern "C" fn getchar()->i32;
fn main()->u64{var value:i32=0;unsafe{value=getchar();}return select(value==49,17u64)}


extern "C" fn putchar(c:i32)->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
package Wide{lead:u8;manages first:Mark;large:i64;small:u16;items:[i32;3];manages last:Mark}
fn make(a:i32,b:i32,n:i32)->Wide=>Wide{lead:7,first:Mark{code:a},large:4294967297,small:513,items:[n,n+1,n+2],last:Mark{code:b}}
fn empty()->Store<Wide> contract{result(create)}{return store_new()}
fn main()->void!i32{
 let s=empty();if !store_reserve(s,2){return ()};
 store_push(s,make(65,97,10));store_push(s,make(66,98,30));
 store_push(s,make(67,99,40));
 let grew=store_reserve(s,8);
 if store_len(s)!=2||(grew&&store_capacity(s)!=8)||(!grew&&store_capacity(s)!=2){return 4};
 let p=store_pop(s)?;
 if p.lead!=7||p.large!=4294967297||p.small!=513||p.items[0]!=30||p.items[1]!=31{return 5};
 return p.items[2]-32
}

import model
import api
extern "C" fn putchar(n:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}
package Buffers{manages b:ByteBuffer;manages c:ByteBuffer}
fn value()->i64{
 let token=Token{code:84};
 var p=Buffers{b:bytes_new(),c:bytes_new()};
 if !bytes_reserve(p.b,4){return -1}
 if !bytes_push(p.b,2u8){return -1}
 let view=bytes_view(p.b);
 let old=give p.c;
 p.c=bytes_new();
 if !bytes_reserve(p.c,16){return -2}
 if !bytes_push(p.c,40u8){return -3}
 let seed=#(model.pack(40));
 let callback=(n:api.Number,s:api.Switch)=>{
  match s{
   model.Flag.N(on)=>{if on{return model.pack(model.unpack(n)+2)}return n},
   model.Flag.Off()=>{return n}
  }
 };
 let answer=api.apply(&callback,seed,model.on());
 return model.unpack(answer)+i64(view[0])+bytes_len(old)-2;
}

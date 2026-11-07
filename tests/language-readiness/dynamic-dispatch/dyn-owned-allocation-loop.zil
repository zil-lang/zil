trait Read{fn read(self:&Self)->i32;}
package A{manages data:ByteBuffer;pub n:i32}
impl Read for A{fn read(self:&Self)->i32{return self.n}}
fn main()->i32{
    var index:i32=0;
    while index<400000{
        let data=bytes_new();
        bytes_reserve(data,64);
        let made=owned_dyn::<dyn Read>(A{data:give data,n:41});
        index+=1;
    }
    return index
}

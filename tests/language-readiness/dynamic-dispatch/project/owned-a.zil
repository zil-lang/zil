pub trait Read{fn read(self:&Self)->i32;}
pub package First{manages data:ByteBuffer;pub n:i32}
impl Read for First{fn read(self:&Self)->i32{return self.n}}
pub fn first()->DynCreateError!OwnedDyn<dyn Read>{
    return owned_dyn::<dyn Read>(First{data:bytes_new(),n:11})
}
pub fn read_owned(owned:OwnedDyn<dyn Read>)->i32{
    return owned.read()
}

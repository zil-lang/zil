import a;
pub package Second{manages data:ByteBuffer;pub n:i32;pub extra:i32}
impl a.Read for Second{fn read(self:&Self)->i32{return self.extra}}
pub fn second()->DynCreateError!OwnedDyn<dyn a.Read>{
    return owned_dyn::<dyn a.Read>(Second{data:bytes_new(),n:17,extra:19})
}

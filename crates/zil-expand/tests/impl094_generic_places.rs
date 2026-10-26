use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};

fn expand(text: &str) -> zil_base::CompileResult<zil_expand::ExpandedModule> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "impl094-generic.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand_with_map_limits(&source, &syntax, &Limits::default())
}

#[test]
fn unused_parametric_places_have_real_symbolic_rules() {
    for text in [
        "fn tail<T>(p:&[T])->&[T]{return &p[1..];}fn main(){}",
        "fn tail<T>(p:&mut[T])->&mut[T]{return &mut p[1..];}fn main(){}",
        "fn put<T>(p:&mut[T],v:T){p[0]=v;}fn main(){}",
        "fn put<T>(p:&mut T,v:T){*p=v;}fn main(){}",
        "fn len<T>(p:&[T])->i64{return p.len();}fn main(){}",
        "fn work<T>(v:T){var p=1;p=2;p+=3;}fn main(){}",
        "fn reborrow<T>(p:&mut T)->&mut T{return &mut *p;}fn main(){}",
        "fn view<T>(p:&[T])->&[T]{return p;}fn tail<T>(p:&[T])->&[T]{return &(view(p))[1..];}fn main(){}",
        "fn put<T>(p:&mut[T],v:T){p[0]=v;}fn work<T>(p:&mut[T],v:T)->i64{put(p,v);return p.len();}fn main(){}",
        "fn len<T>(p:&[T])->i64{return p.len();}fn work<T>(p:&mut[T])->i64{return len(p)+p.len();}fn main(){}",
        "fn work<T>(b:ByteBuffer,v:T)->i64{bytes_reserve(b,2);bytes_push(b,1);return bytes_view(b).len();}fn main(){}",
    ] {
        expand(text).unwrap_or_else(|errors| panic!("{text}: {errors:#?}"));
    }
}

#[test]
fn unused_templates_cannot_invent_mutability_copy_or_index_types() {
    for text in [
        "fn bad<T>(p:&[T],v:T){p[0]=v;}fn main(){}",
        "fn bad<T>(p:&T,v:T){*p=v;}fn main(){}",
        "fn bad<T>(p:T,v:T){p=v;}fn main(){}",
        "fn bad<T>(p:&mut[T],v:T){p[0]=v;p[1]=v;}fn main(){}",
        "fn bad<T>(p:&[T])->T{return p[0];}fn main(){}",
        "fn bad<T>(p:&[T])->&mut[T]{return &mut p[..];}fn main(){}",
        "fn bad<T>(p:&[T])->&[T]{return &p[true..];}fn main(){}",
        "fn bad<T>(p:&mut[T],v:T){p[false]=v;}fn main(){}",
        "fn bad<T>(p:&[T])->i64{return p.len(1);}fn main(){}",
        "fn bad<T>(v:T){let n=1;n=2;}fn main(){}",
        "fn take<T>(v:T){}fn bad<T>(p:&mut[T])->i64{take(p);return p.len();}fn main(){}",
        "fn put<T>(p:&mut[T],v:T){p[0]=v;}fn bad<T>(p:&[T],v:T){put(p,v);}fn main(){}",
        "fn bad<T>(b:ByteBuffer,v:T){bytes_push(b,v);}fn main(){}",
    ] {
        let errors = expand(text).unwrap_err();
        assert_ne!(errors[0].category, ResultCategory::Success, "{text}");
    }
}

#[test]
fn reserved_byte_storage_names_survive_erased_declarations() {
    for text in [
        "type ByteBuffer=i32;fn main(){}",
        "struct ByteBuffer {}fn main(){}",
        "enum ByteBuffer {A}fn main(){}",
        "resource ByteBuffer {x:i32;}fn main(){}",
        "fn f<ByteBuffer>(x:ByteBuffer)->ByteBuffer{return x;}fn main(){}",
        "fn bytes_new(){}fn main(){}",
        "fn f<T>(bytes_push:T){}fn main(){}",
    ] {
        let errors = expand(text).unwrap_err();
        assert_eq!(errors[0].code, "E-BYTE-BUFFER-NAME", "{text}: {errors:#?}");
    }
}

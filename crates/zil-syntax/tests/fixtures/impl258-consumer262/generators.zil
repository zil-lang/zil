import std.meta as meta
import std.syntax as syntax
import common.io as io
import common.model as model

pub fn type_id(t:Type)->Type { return t }
pub fn four(t:Type)->Type { return meta.array_type(type_id(t),4) }
pub fn size_of(t:Type)->i64 { return meta.layout(t).size }
pub fn align_of(t:Type)->i64 { return meta.layout(t).alignment }
pub fn plus_one(s:Syntax<Expr>)->Syntax<Expr> {
    return quote<Expr>{ $(s)+1 }
}
pub fn array_spelling(t:Type,n:i32)->Syntax<Type> {
    return quote<Type>{ [$(syntax.type_ref(t));$(syntax.literal(n))] }
}
pub fn twice(s:Syntax<StmtList>)->Syntax<StmtList> {
    var out=quote<StmtList>{};
    for i in 0..2 {
        out=quote<StmtList>{
            $(out)
            $(s)
        };
    }
    return out
}
pub fn answer(name:Ident)->Syntax<ItemList> {
    return quote<ItemList>{ pub fn $(name)()->i32 { return 42 } }
}
pub fn getter(t:Type,name:Ident)->Syntax<ItemList> {
    let field=meta.field(t,"x");
    return quote<ItemList>{
        pub fn $(name)(value:$(syntax.type_ref(t)))->$(syntax.type_ref(field.ty)) {
            return value.$(field.name)
        }
    }
}
pub fn copy_record(name:Ident)->Syntax<ItemList> {
    return quote<ItemList>{ pub struct $(name) { pub value:i32 } }
}
pub fn owned_package(name:Ident)->Syntax<ItemList> {
    return quote<ItemList>{
        pub package $(name) { manages ticket:model.Ticket; pub n:i32 }
    }
}
pub fn cleanup(name:Ident)->Syntax<ItemList> {
    return quote<ItemList>{
        pub fn $(name)(early:bool)->i32 {
            let first=model.Ticket{code:65};
            if early { return 0 }
            let second=model.Ticket{code:66};
            return 0
        }
    }
}
pub fn fallible(name:Ident,check_name:Ident)->Syntax<ItemList> {
    return quote<ItemList>{
        pub package WorkItem {
            manages ticket:model.Ticket;
            manages data:ByteBuffer;
            pub code:i32;
        }
        pub fn $(name)(stop:bool)->void!WorkItem {
            let ticket=model.Ticket{code:70};
            let data=bytes_new();
            if stop { return () }
            if !bytes_reserve(data,1) || !bytes_push(data,u8(88)) { return () }
            return WorkItem{ticket:give ticket,data:give data,code:88}
        }
        pub fn $(check_name)(item:WorkItem)->i32 {
            if bytes_len(item.data)!=1 || item.code!=88 { return 9 }
            let view=bytes_view(item.data);
            if view[0]!=u8(88) { return 8 }
            return 0
        }
    }
}
pub fn duplicate(s:Syntax<Expr>)->Syntax<StmtList> {
    return quote<StmtList>{ let first=$(s); let second=$(s); }
}
pub fn mismatched_member()->Syntax<Expr> {
    let field=meta.field(model.OtherPoint,"x");
    return quote<Expr>{ (model.Point{x:1,y:2}).$(field.name) }
}

use zil_base::{FileId, ModuleId, Phase, SourceUnit};

#[test]
fn store_names_are_reserved_before_alias_elimination_and_specialization() {
    for source in [
        "struct Store { x:i32 } fn main(){}",
        "resource Store { x:i32 } fn main(){}",
        "resource R{} package Store { manages r:R } fn main(){}",
        "enum Store { One } fn main(){}",
        "trait Store { fn read(self:&Self)->i32; } fn main(){}",
        "type Store = i32; fn main(){}",
        "fn identity<Store>(x:Store)->Store{return x} fn main(){}",
        "type Alias<Store> = Store; fn main(){}",
        "struct Holder<Store>{value:Store} fn main(){}",
        "type store_new = i32; fn main(){}",
    ] {
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "reserved.zil", source);
        let syntax = zil_syntax::parse(&unit).into_result().unwrap();
        let errors = zil_expand::expand(&unit, &syntax).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Expand && d.code == "E-OWNED-STORE-NAME"),
            "{source}: {errors:?}"
        );
    }
}

#[test]
fn ordinary_member_names_do_not_become_reserved_bare_bindings() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "members.zil",
        "struct Fields{Store:i32;store_new:i32}fn main()->i32{let f=Fields{Store:2,store_new:3};return f.Store+f.store_new-5}",
    );
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    zil_expand::expand(&unit, &syntax).unwrap();
}

use zil_base::{FileId, ModuleId, Phase, SourceUnit};

#[test]
fn store_at_is_reserved_before_alias_or_generic_elimination() {
    for source in [
        "fn store_at()->i32{return 0} fn main(){}",
        "type store_at = i32; fn main(){}",
        "struct store_at { x:i32 } fn main(){}",
        "fn id<store_at>(x:store_at)->store_at{return x} fn main(){}",
        "struct Holder<store_at>{x:store_at} fn main(){}",
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
fn store_at_member_is_an_ordinary_field() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "member.zil",
        "struct Fields{store_at:i32}fn main()->i32{let f=Fields{store_at:3};return f.store_at-3}",
    );
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    zil_expand::expand(&unit, &syntax).unwrap();
}

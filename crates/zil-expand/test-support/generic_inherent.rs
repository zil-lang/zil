use zil_base::{FileId, Limits, ModuleId, Phase, SourceUnit};

fn expand(text: &str) -> Result<zil_expand::ExpandedModule, Vec<zil_base::Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "inherent.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand_with_map_limits(&source, &ast, &Limits::default())
}

#[test]
fn ordinary_generic_inherent_methods_use_the_nominal_receiver() {
    for source in [
        "struct Box<T:Copy>{value:T} impl<T:Copy> Box<T>{fn get(self:&Self)->T{return self.value}} fn result()->i32{let x=Box::<i32>{value:42};return x.get()}",
        "struct Box<T:Copy>{value:T} impl<T:Copy> Box<T>{fn replace(self:&mut Self,value:T)->T{let old=self.value;self.value=value;return old}} fn result()->i32{var x=Box::<i32>{value:42};return x.replace(7)}",
        "struct Box<T:Copy>{value:T} impl<T:Copy> Box<T>{fn get(self:&Self)->T{return self.value}} fn result()->i32{let x=Box::<i32>{value:42};let y=Box::<bool>{value:true};if y.get(){return x.get()};return 0}",
    ] {
        let core = expand(source).unwrap_or_else(|errors| panic!("{source}\n{errors:#?}"));
        assert!(
            core.core.unsupported.is_empty(),
            "{:?}",
            core.core.unsupported
        );
        assert!(
            core.core
                .functions
                .iter()
                .any(|f| f.name.contains("inherent"))
        );
    }
}

#[test]
fn unused_inherent_methods_check_symbolic_bounds_and_receiver_fields() {
    for source in [
        "struct Box<T>{value:T} impl<T> Box<T>{fn bad(self:&Self)->T{return self.value+1}} fn main()->i32{return 0}",
        "struct Box<T:Copy>{value:T} impl<T:Copy> Box<T>{fn bad(self:&Self)->i32{return self.value.missing}} fn main()->i32{return 0}",
        "struct Box<T:Copy>{value:T} impl<T:Copy> Box<T>{fn bad(self:&Self)->T{self.value=self.value;return self.value}} fn main()->i32{return 0}",
    ] {
        let errors = expand(source).unwrap_err();
        assert!(
            errors.iter().any(|e| e.phase == Phase::Expand),
            "{errors:#?}"
        );
    }
}

#[test]
fn inherent_specialization_and_duplicate_method_do_not_erase_obligations() {
    for source in [
        "struct Box<T:Copy>{value:T} impl<T:Copy> Box<i32>{fn get(self:&Self)->i32{return self.value}} fn main()->i32{return 0}",
        "struct Box<T:Copy>{value:T} impl<T:Copy> Box<T>{fn get(self:&Self)->T{return self.value}} impl<T:Copy> Box<T>{fn get(self:&Self)->T{return self.value}} fn main()->i32{return 0}",
    ] {
        assert!(expand(source).is_err(), "{source}");
    }
}

#[test]
fn stored_slice_cursor_method_keeps_its_impl_life_and_nominal_type() {
    let source = "struct Cursor<life s,T:Copy>{slice:&'s [T];index:i64} fn iter<life s,T:Copy>(slice:&'s [T])->Cursor<'s,T>{return Cursor::<'s,T>{slice:slice,index:0}} impl<life s,T:Copy> Cursor<'s,T>{fn remaining(self:&Self)->i64{return self.slice.len()-self.index}} fn result()->i64{let values:[i64;3]=[10,20,12];let cursor=iter(&values[..]);return cursor.remaining()}";
    let core = expand(source).unwrap_or_else(|errors| panic!("{errors:#?}"));
    assert!(
        core.core.unsupported.is_empty(),
        "{:?}",
        core.core.unsupported
    );
}

#[test]
fn two_life_receiver_with_reordered_impl_binders_keeps_source_positions() {
    let source = "struct Pair<life left,life right,T:Copy>{left:&'left T;right:&'right T} impl<life right,T:Copy,life left> Pair<'left,'right,T>{fn both(self:&Self)->Pair<'left,'right,T>{return Pair::<'left,'right,T>{left:self.left,right:self.right}}} fn pair<life left,life right,T:Copy>(a:&'left T,b:&'right T)->Pair<'left,'right,T>{return Pair::<'left,'right,T>{left:a,right:b}} fn result()->i64{let a=20i64;let b=22i64;let p=pair(&a,&b);let copy=p.both();return *copy.left+*copy.right}";
    let core = expand(source).unwrap_or_else(|errors| panic!("{errors:#?}"));
    assert!(
        core.core.unsupported.is_empty(),
        "{:?}",
        core.core.unsupported
    );
    let wrong = source.replace(
        "Pair::<'left,'right,T>{left:self.left,right:self.right}",
        "Pair::<'right,'left,T>{left:self.right,right:self.left}",
    );
    let errors = expand(&wrong).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.phase == Phase::Expand && e.message.contains("class")),
        "{errors:#?}"
    );
}

#[path = "budget_boundary.rs"]
mod budget_boundary;

#[test]
fn inherent_specialization_preserves_exact_request_boundaries() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "inherent-budget.zil",
        "struct Box<T:Copy>{value:T} impl<T:Copy> Box<T>{fn get(self:&Self)->T{return self.value}} fn result()->i32{let x=Box::<i32>{value:42};return x.get()}",
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let expected = zil_expand::expand_with_map_limits(&source, &ast, &Limits::default())
        .unwrap()
        .core;
    for work in [true, false] {
        let amount = budget_boundary::minimum(work, |limits| {
            zil_expand::expand_with_map_limits(&source, &ast, limits)
        });
        eprintln!("inherent budget work={work} N={amount}");
        assert_eq!(amount, if work { 1418 } else { 483 });
        let mut limits = Limits::default();
        if work {
            limits.max_analysis_steps = amount;
        } else {
            limits.max_nodes = amount;
        }
        let fresh = zil_syntax::parse(&source).into_result().unwrap();
        for syntax in [&ast, &fresh] {
            assert_eq!(
                zil_expand::expand_with_map_limits(&source, syntax, &limits)
                    .unwrap()
                    .core,
                expected
            );
        }
        if work {
            limits.max_analysis_steps -= 1;
        } else {
            limits.max_nodes -= 1;
        }
        for syntax in [&ast, &fresh] {
            assert!(
                zil_expand::expand_with_map_limits(&source, syntax, &limits)
                    .unwrap_err()
                    .iter()
                    .all(|e| e.category == zil_base::ResultCategory::ResourceLimit)
            );
        }
    }
}

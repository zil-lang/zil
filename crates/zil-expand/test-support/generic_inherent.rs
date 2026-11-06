use zil_base::{FileId, Limits, ModuleId, Phase, SourceUnit};

fn expand(text: &str) -> Result<zil_expand::ExpandedModule, Vec<zil_base::Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "inherent.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand_with_map_limits_target_context(
        &source,
        &ast,
        &Limits::default(),
        &zil_base::TargetProfile::default(),
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: ModuleId(0),
            aliases: &std::collections::BTreeMap::new(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
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
        "struct Box<T:Copy>{get:T} impl<T:Copy> Box<T>{fn get(self:&Self)->T{return self.get}} fn main()->i32{return 0}",
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
    let source = "struct Pair<life left,life right,T:Copy>{left:&'left T;right:&'right T} impl<life right,T:Copy,life left> Pair<'left,'right,T>{fn both(self:&Self)->Pair<'left,'right,T>{return Pair::<'left,'right,T>{left:self.left,right:self.right}}} fn pair<life left,life right,T:Copy>(a:&'left T,b:&'right T)->Pair<'left,'right,T>{return Pair::<'left,'right,T>{left:a,right:b}} fn result()->i64{let a:i64=20;let b:i64=22;let p=pair(&a,&b);let copy=p.both();return *copy.left+*copy.right}";
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

use super::budget_boundary;

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
        // Measured in the combined b0839a97 Source-to-Core run.
        assert_eq!(amount, if work { 1431 } else { 484 });

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

#[test]
fn generic_inherent_visibility_uses_the_declaring_module() {
    for (owner_public, public, factory) in [
        (true, true, false),
        (true, false, false),
        (false, true, true),
    ] {
        let declaration = format!(
            "{}struct Box<T:Copy>{{pub value:T}} impl<T:Copy> Box<T>{{{}fn get(self:&Self)->T{{return self.value}}}} pub fn make()->Box<i32>{{return Box::<i32>{{value:42}}}}",
            if owner_public { "pub " } else { "" },
            if public { "pub " } else { "" }
        );
        let sources = [
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                if factory {
                    "import data\nfn result()->i32{let value=data.make();return value.get()}"
                } else {
                    "import data\nfn result()->i32{let value=data.Box::<i32>{value:42};return value.get()}"
                },
            ),
            SourceUnit::new(FileId(1), ModuleId(1), "data.zil", declaration),
        ];
        let syntax = sources
            .iter()
            .map(|source| zil_syntax::parse(source).into_result().unwrap())
            .collect::<Vec<_>>();
        let imports = [
            vec![zil_ir::CoreImport {
                alias: "data".into(),
                module: ModuleId(1),
                span: syntax[0].span,
            }],
            vec![],
        ];
        let aliases = std::collections::BTreeMap::from([(vec!["data".into()], ModuleId(1))]);
        let result = zil_expand::expand_program_with_limits_target_context(
            &sources,
            &syntax.iter().collect::<Vec<_>>(),
            &imports,
            &Limits::default(),
            &zil_base::TargetProfile::default(),
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &aliases,
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        );
        if owner_public && public {
            result.unwrap_or_else(|errors| panic!("{errors:#?}"));
        } else {
            let errors = result.unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.phase == Phase::Expand && error.message.contains("private")),
                "{errors:#?}"
            );
        }
    }
}

#[test]
fn life_only_inherent_member_keeps_the_ordinary_retained_function_path() {
    let source = "struct Cursor<life s>{slice:&'s[i64]} impl<life s> Cursor<'s>{fn len(self:&Self)->i64{return self.slice.len()}} fn make<life s>(p:&'s[i64])->Cursor<'s>{return Cursor::<'s>{slice:p}} fn result()->i64{let a:[i64;1]=[42];let c=make(&a[..]);return c.len()}";
    let core = expand(source).unwrap_or_else(|errors| panic!("{errors:#?}"));
    assert!(
        core.core.unsupported.is_empty(),
        "{:?}",
        core.core.unsupported
    );
}

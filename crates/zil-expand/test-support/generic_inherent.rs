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

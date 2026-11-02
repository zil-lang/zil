//! Closed CT services over the pre-generation source snapshot. Host callbacks
//! return pure data and never execute a provider or grant ordinary Verify.
use super::{snapshot::ShapeSnapshot, source_index::SourceIndex, type_ref::from_checked_type};
use zil_base::{CompileResult, Diagnostic, Fingerprint, Phase, SourceUnit};
use zil_ct::{CtExecutionBudget, CtReadView};
use zil_ir::{Type, ct_artifact::*};
use zil_syntax::ast::{self as s, ItemKind};

fn error(code: &str, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, code, None, message)]
}
fn phase(message: &str) -> Vec<Diagnostic> { error("E-CT-PHASE-DEPENDENCY", message) }

pub(super) struct FrozenReadView<'a> {
    pub index: &'a SourceIndex,
    pub sources: &'a [SourceUnit],
    pub syntax: &'a [&'a s::Module],
    pub shape: Option<&'a ShapeSnapshot>,
    pub input: Option<&'a CtTokenTree>,
}

impl FrozenReadView<'_> {
    fn declaration(&self, symbol: &CtSymbolId) -> Option<&s::Item> {
        self.syntax.iter().find(|module| module.id == symbol.module)?.items.get(symbol.declaration.0 as usize)
    }
    fn nominal(&self, requester: &CtSymbolId, symbol: &CtSymbolId) -> CompileResult<&s::Item> {
        if !self.index.visible(requester, symbol) { return Err(error("E-CT-VISIBILITY", "CT type declaration is absent or hidden at this definition site")); }
        let item = self.declaration(symbol).ok_or_else(|| phase("type declaration is not in the frozen source batch"))?;
        if !matches!(item.kind, ItemKind::Aggregate(_) | ItemKind::Enum(_) | ItemKind::TypeAlias(_)) { return Err(phase("symbol is not a ready nominal type")); }
        let ready = self.shape.ok_or_else(|| phase("ShapeReady has not completed"))?;
        let found = ready.core.modules.iter().any(|module| module.core.module_id == symbol.module && (
            module.core.aggregates.iter().any(|aggregate| aggregate.span == item.span)
            || module.core.enums.iter().any(|enumeration| enumeration.span == item.span)
        ));
        if !found { return Err(phase("type shape depends on a generated or unfinished declaration")); }
        Ok(item)
    }
    fn nominal_root<'a>(&self, value: &'a CtValue) -> CompileResult<(&'a CtTypeRef, &'a CtSymbolId)> {
        let CtValue::Type(ty) = value else { return Err(error("E-CT-QUERY-TYPE", "reflection requires a Type value")); };
        let Some(CtTypeNode::Nominal { symbol, .. }) = ty.nodes.get(ty.root.0 as usize) else { return Err(error("E-CT-QUERY-TYPE", "reflection requires a nominal type")); };
        Ok((ty, symbol))
    }
    fn aggregate(&self, symbol: &CtSymbolId) -> CompileResult<&zil_ir::CoreAggregate> {
        let ready = self.shape.ok_or_else(|| phase("ShapeReady has not completed"))?;
        let span = self.index.span(symbol).ok_or_else(|| phase("shape symbol is absent"))?;
        ready.core.modules.iter().find(|module| module.core.module_id == symbol.module)
            .and_then(|module| module.core.aggregates.iter().find(|aggregate| aggregate.span == span))
            .ok_or_else(|| phase("nominal has no completed aggregate shape"))
    }
    fn field_info(&self, requester: &CtSymbolId, owner: &CtSymbolId, field: &zil_ir::CoreField, budget: &mut CtExecutionBudget) -> CompileResult<CtFieldInfo> {
        if requester.module != owner.module && !field.public { return Err(error("E-CT-VISIBILITY", "field is private at this CT definition site")); }
        let field_symbol = self.index.symbol_at(owner.module, field.span).ok_or_else(|| phase("field identity is absent from frozen source"))?;
        let ready = self.shape.ok_or_else(|| phase("ShapeReady has not completed"))?;
        budget.reserve_analysis_work(1)?;
        budget.reserve_generated_nodes(2)?;
        budget.reserve_temporary_bytes(field.name.len() as u64)?;
        let ty = from_checked_type(&field.ty, ready, self.index, budget.remaining().max_generated_nodes as usize)?;
        Ok(CtFieldInfo {
            owner: owner.clone(), field: field_symbol.clone(),
            name: CtIdent { text: field.name.clone(), namespace: CtNameNamespace::Member,
                source: CtNameSource::Definition(field_symbol), member_owner: Some(owner.clone()) },
            ty, managed: field.manages,
        })
    }
    fn shape_info(&self, requester: &CtSymbolId, owner: &CtSymbolId, budget: &mut CtExecutionBudget) -> CompileResult<CtShapeInfo> {
        self.nominal(requester, owner)?;
        let aggregate = self.aggregate(owner)?;
        let mut fields = Vec::new();
        let mut complete = true;
        for field in &aggregate.fields {
            budget.reserve_analysis_work(1)?;
            if requester.module == owner.module || field.public {
                fields.push(self.field_info(requester, owner, field, budget)?);
            } else { complete = false; }
        }
        budget.reserve_generated_nodes(1)?;
        Ok(CtShapeInfo { nominal: owner.clone(), visible_fields: fields, complete })
    }
    fn dependency(kind: CtQueryKind, input: &[CtValue], output: &CtValue) -> CtQueryDependency {
        CtQueryDependency { kind, subject_fingerprint: Fingerprint::of(format!("{input:?}").as_bytes()),
            result_fingerprint: Fingerprint::of(format!("{output:?}").as_bytes()) }
    }
    fn text_name(&self, symbol: &CtSymbolId) -> CompileResult<(String, CtNameNamespace)> {
        let item = self.declaration(symbol).ok_or_else(|| phase("definition is absent from source index"))?;
        Ok(match &item.kind {
            ItemKind::Function(f) => (f.name.clone(), CtNameNamespace::Value),
            ItemKind::Aggregate(a) => (a.name.clone(), CtNameNamespace::Type),
            ItemKind::Enum(e) => (e.name.clone(), CtNameNamespace::Type),
            ItemKind::Trait(t) => (t.name.clone(), CtNameNamespace::Type),
            ItemKind::TypeAlias(t) => (t.name.clone(), CtNameNamespace::Type),
            ItemKind::Const(c) => (c.name.clone(), CtNameNamespace::Value),
            _ => return Err(phase("definition has no CT name")),
        })
    }
}

impl CtReadView for FrozenReadView<'_> {
    fn authenticate_argument(&self, index: u32, ty: &CtTy, value: &CtValue) -> CompileResult<()> {
        match (ty, value) {
            (CtTy::TokenTree, CtValue::TokenTree(tree)) if index == 0 && self.input == Some(tree) => Ok(()),
            (CtTy::TokenTree, _) => Err(error("E-CT-INPUT", "TokenTree argument is not the authenticated provider input")),
            (CtTy::Type, CtValue::Type(_)) | (CtTy::Syntax(_), CtValue::Syntax(_)) | (CtTy::Ident, CtValue::Ident(_)) => Ok(()),
            (CtTy::Scalar(_), CtValue::Scalar(_)) | (CtTy::Text, CtValue::Text(_)) => Ok(()),
            _ => Err(error("E-CT-INPUT", "CT argument category differs from the declared parameter")),
        }
    }
    fn type_name(&self, requester: &CtSymbolId, symbol: &CtSymbolId, budget: &mut CtExecutionBudget) -> CompileResult<CtTypeRef> {
        self.nominal(requester, symbol)?;
        budget.reserve_analysis_work(1)?;
        budget.reserve_generated_nodes(1)?;
        Ok(CtTypeRef { nodes: vec![CtTypeNode::Nominal { symbol: symbol.clone(), arguments: Vec::new() }], root: CtTypeNodeId(0) })
    }
    fn type_apply(&self, requester: &CtSymbolId, nominal: &CtSymbolId, args: &[CtValue], budget: &mut CtExecutionBudget) -> CompileResult<CtTypeRef> {
        self.nominal(requester, nominal)?;
        if !args.is_empty() { return Err(phase("generic type application has no completed shape in this batch")); }
        self.type_name(requester, nominal, budget)
    }
    fn definition_ident(&self, requester: &CtSymbolId, symbol: &CtSymbolId, budget: &mut CtExecutionBudget) -> CompileResult<CtIdent> {
        if !self.index.visible(requester, symbol) { return Err(error("E-CT-VISIBILITY", "definition name is private")); }
        let (text, namespace) = self.text_name(symbol)?;
        budget.reserve_analysis_work(1)?;
        budget.reserve_generated_nodes(1)?;
        budget.reserve_temporary_bytes(text.len() as u64)?;
        Ok(CtIdent { text, namespace, source: CtNameSource::Definition(symbol.clone()), member_owner: None })
    }
    fn parse(&self, _category: CtSyntaxCategory, _input: &CtTokenTree, _budget: &mut CtExecutionBudget) -> CompileResult<CtSyntaxTree> {
        Err(phase("generated syntax parser is not connected to the frozen input view"))
    }
    fn query(&self, requester: &CtSymbolId, kind: CtQueryKind, args: &[CtValue], budget: &mut CtExecutionBudget) -> CompileResult<(CtValue, CtQueryDependency)> {
        let value = match kind {
            CtQueryKind::Shape => {
                let (_, nominal) = self.nominal_root(args.first().ok_or_else(|| error("E-CT-QUERY-ARG", "shape query needs a type"))?)?;
                CtValue::ShapeInfo(self.shape_info(requester, nominal, budget)?)
            }
            CtQueryKind::Field => {
                let (_, nominal) = self.nominal_root(args.first().ok_or_else(|| error("E-CT-QUERY-ARG", "field query needs a type"))?)?;
                self.nominal(requester, nominal)?;
                let name = match args.get(1) { Some(CtValue::Text(name)) => name.as_str(), _ => return Err(error("E-CT-QUERY-ARG", "field query needs a text name")) };
                let field = self.aggregate(nominal)?.fields.iter().find(|field| field.name == name)
                    .ok_or_else(|| error("E-CT-FIELD-NOT-FOUND", "field is not in the completed shape"))?;
                CtValue::FieldInfo(self.field_info(requester, nominal, field, budget)?)
            }
            CtQueryKind::ImplIndex => return Err(phase("ImplIndexReady has not completed for this generation batch")),
            CtQueryKind::Layout => return Err(phase("LayoutReady has not completed for this generation batch")),
        };
        let dependency = Self::dependency(kind, args, &value);
        Ok((value, dependency))
    }
    fn build_syntax(&self, _kind: CtSyntaxBuilder, _args: &[CtValue], _budget: &mut CtExecutionBudget) -> CompileResult<CtSyntaxTree> {
        Err(phase("generated syntax construction has not been materialized"))
    }
    fn instantiate_quote(&self, _template: &CtTemplate, _holes: &[CtValue], _budget: &mut CtExecutionBudget) -> CompileResult<CtSyntaxTree> {
        Err(phase("quote hygienic instantiation has not completed"))
    }
    fn token_inspect(&self, _source: &CtValue, _projection: CtTokenProjection, _index: Option<i64>, _budget: &mut CtExecutionBudget) -> CompileResult<CtValue> {
        Err(phase("provider token cursor is not connected to authenticated input"))
    }
}

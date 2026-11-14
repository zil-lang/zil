//! A conservative, immutable index of original closure name syntax. It only
//! avoids copying unrelated creator bindings; Gamma still derives captures.
use super::*;

pub(super) struct Candidates {
    pub names: BTreeSet<String>,
    pub opaque_type_syntax: bool,
}
struct Collect<'a> {
    result: Candidates,
    limits: &'a Limits,
    left: &'a mut usize,
}
impl Collect<'_> {
    fn pay(&mut self, work: usize, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(work).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original capture syntax index work exceeded",
                None,
            )
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original capture syntax index depth exceeded",
                None,
            ));
        }
        Ok(())
    }
}
impl DynRecipeVisitor for Collect<'_> {
    type Error = Diagnostic;
    fn node(&mut self, depth: usize) -> R<()> {
        self.pay(1, depth)
    }
    fn rows(&mut self, n: usize, depth: usize) -> R<()> {
        self.pay(n.saturating_add(1), depth)
    }
    fn scratch(&mut self, bytes: usize, depth: usize) -> R<()> {
        self.pay(
            bytes / std::mem::size_of::<usize>() + usize::from(bytes != 0),
            depth,
        )
    }
    fn text(&mut self, text: &str, depth: usize) -> R<()> {
        self.pay(
            text.len()
                .saturating_add(self.result.names.len().saturating_add(1).ilog2() as usize + 2),
            depth,
        )?;
        if self.result.names.len() >= self.limits.max_nodes && !self.result.names.contains(text) {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original capture syntax name count exceeded",
                None,
            ));
        }
        self.result.names.insert(text.to_owned());
        Ok(())
    }
    fn ty(&mut self, ty: &Type, depth: usize) -> R<()> {
        self.pay(1, depth)?;
        if let Type::Named(name) = ty {
            return self.text(name, depth);
        }
        // A compound opaque Type callback may hide more names. In that case
        // retain every creator binder, preserving the old complete behavior;
        // this optimization never guesses a type's free-name grammar.
        if !ty.is_b0_scalar()
            && !matches!(
                ty,
                Type::Unit
                    | Type::Closure(_)
                    | Type::Dyn { .. }
                    | Type::OwnedDyn { .. }
                    | Type::ByteBuffer
                    | Type::TaskI32
                    | Type::ForeignHandle
                    | Type::SyncCell(_)
                    | Type::CWideString
            )
        {
            self.result.opaque_type_syntax = true;
        }
        Ok(())
    }
    fn life(&mut self, _: &StorageLifeShape, depth: usize) -> R<()> {
        self.pay(1, depth)
    }
    fn identity(&mut self, _: &LifeDeclarationIdentity, depth: usize) -> R<()> {
        self.pay(1, depth)
    }
    fn position(&mut self, _: &StoragePosition, depth: usize) -> R<()> {
        self.pay(1, depth)
    }
    fn constant(&mut self, _: &Constant, depth: usize) -> R<()> {
        self.pay(1, depth)
    }
}
impl super::super::original_walk::Context for Collect<'_> {
    fn node(&mut self, depth: usize) -> R<()> {
        self.pay(1, depth)
    }
    fn ty(&mut self, value: &mut SemanticTypeRecipe, depth: usize) -> R<()> {
        value.visit_dyn_metadata(self, depth)
    }
    fn expression(&mut self, _: NodeId, kind: &SemanticOriginalExprKind, depth: usize) -> R<()> {
        self.pay(1, depth)?;
        match kind {
            CoreExprKind::Name(name) => DynRecipeVisitor::text(self, name, depth)?,
            // These contexts may introduce or resolve names through a stage or
            // inherited receiver. Keep the complete creator environment rather
            // than deriving their lexical semantics from a textual index.
            CoreExprKind::CompileTime(_)
            | CoreExprKind::CompileTimeBlock(_)
            | CoreExprKind::SyntaxQuote { .. }
            | CoreExprKind::SyntaxIsland { .. }
            | CoreExprKind::Splice(_)
            | CoreExprKind::ReceiverMember(_)
            | CoreExprKind::Unsupported(_) => self.result.opaque_type_syntax = true,
            _ => {}
        }
        Ok(())
    }
}
pub(super) fn collect(
    definition: &SemanticDefinition,
    limits: &Limits,
    left: &mut usize,
) -> R<std::rc::Rc<Candidates>> {
    let SemanticDefinitionBody::FunctionWithBody {
        signature,
        parameters,
        body,
        ..
    } = &definition.body
    else {
        return Err(bad(
            "original closure capture index requires its complete original body",
        ));
    };
    let mut collect = Collect {
        result: Candidates {
            names: BTreeSet::new(),
            opaque_type_syntax: false,
        },
        limits,
        left,
    };
    // Includes nested bodies, all control branches, staged syntax, annotations
    // and original environment/query names. Names shadowed by child locals may
    // remain in this superset; independent Gamma decides their actual use.
    if let SemanticOriginalFunctionBody::Defined(body) = body {
        // One paid syntax snapshot per immutable child index, never a deep
        // snapshot of every unrelated creator binding at every occurrence.
        crate::dyn_metadata::semantic_operand_snapshot(body, limits, collect.left, 1)?;
        let mut body = body.clone();
        super::super::original_walk::block(&mut body, &mut collect, 0)?;
    }
    signature.visit_dyn_metadata(&mut collect, 0)?;
    for parameter in parameters {
        parameter.visit_dyn_metadata(&mut collect, 0)?;
    }
    for formal in &definition.environment {
        formal.visit_dyn_metadata(&mut collect, 0)?;
    }
    for formal in &definition.parameters {
        formal.visit_dyn_metadata(&mut collect, 0)?;
    }
    for requirement in &definition.requirements {
        requirement.visit_dyn_metadata(&mut collect, 0)?;
    }
    // Receiver shorthand must not make a parent receiver disappear from the
    // lexical environment even where the original child has no ordinary Name.
    collect.text("self", 0)?;
    collect.pay(1, 0)?;
    Ok(std::rc::Rc::new(collect.result))
}

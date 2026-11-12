//! Immutable provenance from an actual parser invocation, without verification authority.
use std::sync::Arc;

use crate::{ParseResult, attributes::AttributeContext, parse_with_attribute_context};
use zil_base::{Limits, SourceUnit};

/// The source and parsed result can only be paired by running the real parser.
/// Read-only access and COW sharing cannot change the syntax of an old revision.
#[derive(Clone, Debug)]
pub struct ParsedSource {
    source: Arc<SourceUnit>,
    parsed: Arc<ParseResult>,
    limits: Limits,
    context: AttributeContext,
}

impl ParsedSource {
    pub fn parse(source: Arc<SourceUnit>, limits: &Limits, context: &AttributeContext) -> Self {
        let parsed = Arc::new(parse_with_attribute_context(&source, limits, context));
        Self {
            source,
            parsed,
            limits: limits.clone(),
            context: context.clone(),
        }
    }

    pub fn source(&self) -> &SourceUnit {
        &self.source
    }

    pub fn parsed(&self) -> &ParseResult {
        &self.parsed
    }

    pub fn parsed_shared(&self) -> Arc<ParseResult> {
        self.parsed.clone()
    }

    /// Grammar/attribute inputs for an already successful fragment proof.
    /// Quotas remain admission checks: callers must use actual current parse
    /// success, actual old/current source bytes and their current stage budgets.
    /// This does not relax matches_current or authorize arbitrary AST equality.
    pub fn same_attribute_context(&self, other: &Self) -> bool {
        self.context == other.context
    }

    /// Equal parser settings only; this never establishes source/AST equality.
    /// Incremental producers separately compare their actual source fragments.
    pub fn same_parse_configuration(&self, other: &Self) -> bool {
        self.limits == other.limits && self.context == other.context
    }

    pub fn same_revision(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.parsed, &other.parsed)
    }

    /// Reuse is conservative across every parser quota and attribute input.
    /// This comparison reads actual source bytes; revision identity does not
    /// turn source ingress into a constant-time operation.
    pub fn matches_current(
        &self,
        source: &SourceUnit,
        limits: &Limits,
        context: &AttributeContext,
    ) -> bool {
        self.source.as_ref() == source
            && self.limits == *limits
            && self.context == *context
            && self.parsed.fits_limits(limits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId};

    #[test]
    fn revision_binds_actual_source_and_cow_cannot_replace_it() {
        let source = Arc::new(SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            "fn f() {}",
        ));
        let limits = Limits::default();
        let context = AttributeContext::default();
        let parsed = ParsedSource::parse(source.clone(), &limits, &context);
        assert!(parsed.matches_current(&source, &limits, &context));
        assert!(parsed.same_revision(&parsed.clone()));
        assert!(!parsed.same_revision(&ParsedSource::parse(source.clone(), &limits, &context)));
        let mut raw = parsed.parsed_shared();
        Arc::make_mut(&mut raw).module.items.clear();
        assert!(!parsed.parsed().module.items.is_empty());
        let mut changed = (*source).clone();
        changed.text = "fn g() {}".into();
        assert!(!parsed.matches_current(&changed, &limits, &context));
        let mut changed_limits = limits.clone();
        changed_limits.max_call_depth += 1;
        assert!(!parsed.matches_current(&source, &changed_limits, &context));
        let mut changed_context = context.clone();
        changed_context
            .configuration
            .insert("debug".into(), crate::attributes::ConfigValue::Bool(true));
        assert!(!parsed.matches_current(&source, &limits, &changed_context));
    }

    #[test]
    fn failed_parse_is_never_a_reusable_success() {
        let source = Arc::new(SourceUnit::new(FileId(0), ModuleId(0), "bad.zil", "fn"));
        let limits = Limits::default();
        let context = AttributeContext::default();
        let parsed = ParsedSource::parse(source.clone(), &limits, &context);
        assert!(!parsed.parsed().is_ok());
        assert!(!parsed.matches_current(&source, &limits, &context));
    }
}

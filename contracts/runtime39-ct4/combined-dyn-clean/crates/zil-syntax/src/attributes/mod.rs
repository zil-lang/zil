//! Attribute syntax validation against the frozen registry. This module does
//! not run attributes, resolve traits/lints, evaluate layout constants, or grant
//! capabilities. Validation of non-cfg attributes waits until an attachment is
//! known to survive configuration and enclosing compile-time selection.
mod config;
mod registry;

use crate::ast::*;
pub use registry::{BUILTIN_ATTRIBUTES, REGISTRY_SHA256};
use std::collections::{BTreeMap, BTreeSet};
use zil_base::{CompileResult, Diagnostic, Limits, Phase, ResultCategory, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttributeTarget {
    Item,
    Function,
    Struct,
    Enum,
    Resource,
    Package,
    Trait,
    Impl,
    TypeAlias,
    Constant,
    Field,
    Variant,
    Parameter,
}
impl AttributeTarget {
    fn is_item(self) -> bool {
        !matches!(self, Self::Field | Self::Variant | Self::Parameter)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatRule {
    And,
    Concat,
    Union,
    Error,
    MergeUniqueTraits,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentRule {
    Configuration,
    Representation,
    Alignment,
    TraitPaths,
    OptionalInline,
    String,
    LintPaths,
    None,
}
#[derive(Clone, Copy, Debug)]
pub struct BuiltinAttributeSpec {
    pub name: &'static str,
    pub phase: u8,
    pub targets: &'static [AttributeTarget],
    pub repeat: RepeatRule,
    pub arguments: ArgumentRule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigValue {
    Bool(bool),
    String(String),
    Integer(i128),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomAttributeKind {
    Syntax,
    Generate,
    Metadata,
}
#[derive(Clone, Debug)]
pub struct CustomAttribute {
    pub path: Vec<String>,
    pub kind: CustomAttributeKind,
    pub targets: Vec<AttributeTarget>,
    pub repeat: RepeatRule,
}
#[derive(Clone, Debug, Default)]
pub struct AttributeContext {
    /// Frozen values explicitly supplied by the build/configuration driver.
    pub configuration: BTreeMap<String, ConfigValue>,
    custom: BTreeMap<Vec<String>, CustomAttribute>,
}
impl AttributeContext {
    /// Register already-loaded manifest data. This never discovers imports,
    /// performs I/O, runs an attribute, or manufactures a verified artifact.
    pub fn register_custom(&mut self, attribute: CustomAttribute) -> CompileResult<()> {
        let valid_name = attribute.path.len() > 1
            && attribute.path.iter().all(|part| {
                let mut chars = part.chars();
                chars.next().is_some_and(crate::unicode::is_xid_start)
                    && chars.all(crate::unicode::is_xid_continue)
            });
        if !valid_name || attribute.targets.is_empty() || self.custom.contains_key(&attribute.path)
        {
            return Err(vec![Diagnostic::error(
                Phase::Parse,
                "E-PARSE-ATTRIBUTE-REGISTRY",
                None,
                "custom attribute must have a unique qualified name and declared targets",
            )]);
        }
        self.custom.insert(attribute.path.clone(), attribute);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeferredReason {
    Configuration,
    CompileTimeSelection,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeferredAttribute {
    pub attachment: Span,
    pub reason: DeferredReason,
}
#[derive(Clone, Debug, Default)]
pub struct AttributeValidation {
    pub diagnostics: Vec<Diagnostic>,
    pub deferred: Vec<DeferredAttribute>,
    /// Source-shaped ASTs retain these attachments; a later configuration
    /// materializer is responsible for removal before names/types/expansion.
    pub removed: Vec<Span>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Retention {
    Keep,
    Remove,
    Pending,
}

pub fn validate(
    module: &Module,
    context: &AttributeContext,
    limits: &Limits,
) -> AttributeValidation {
    validate_with_usage(module, context, limits).0
}

pub(crate) fn validate_with_usage(
    module: &Module,
    context: &AttributeContext,
    limits: &Limits,
) -> (AttributeValidation, usize) {
    let mut validator = Validator {
        context,
        limits,
        result: AttributeValidation::default(),
        work: 0,
        depth: 0,
    };
    for item in &module.items {
        validator.item(item, Retention::Keep);
    }
    (validator.result, validator.work)
}
struct Validator<'a> {
    context: &'a AttributeContext,
    limits: &'a Limits,
    result: AttributeValidation,
    work: usize,
    depth: usize,
}
impl Validator<'_> {
    fn error(&mut self, code: &str, span: Span, message: impl Into<String>) {
        self.result
            .diagnostics
            .push(Diagnostic::error(Phase::Parse, code, Some(span), message));
    }
    fn budget(&mut self, span: Span) -> bool {
        self.work = self.work.saturating_add(1);
        if self.work > self.limits.max_nodes || self.depth >= self.limits.max_call_depth {
            if !self
                .result
                .diagnostics
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit)
            {
                self.result.diagnostics.push(Diagnostic::limit(
                    Phase::Parse,
                    "attribute validation budget exceeded",
                    Some(span),
                ));
            }
            return false;
        }
        true
    }
    fn item(&mut self, item: &Item, parent: Retention) {
        if !self.budget(item.span) {
            return;
        }
        self.depth += 1;
        let target = match &item.kind {
            ItemKind::Function(_) | ItemKind::Extern { .. } => AttributeTarget::Function,
            ItemKind::Aggregate(a) => match a.kind {
                AggregateKind::Struct => AttributeTarget::Struct,
                AggregateKind::Resource => AttributeTarget::Resource,
                AggregateKind::Package => AttributeTarget::Package,
            },
            ItemKind::Enum(_) => AttributeTarget::Enum,
            ItemKind::Trait(_) => AttributeTarget::Trait,
            ItemKind::Impl(_) => AttributeTarget::Impl,
            ItemKind::TypeAlias(_) => AttributeTarget::TypeAlias,
            ItemKind::Const(_) => AttributeTarget::Constant,
            _ => AttributeTarget::Item,
        };
        let retention = self.attachment(&item.attributes, target, item.span, parent);
        if retention != Retention::Remove {
            match &item.kind {
                ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => {
                    self.parameters(f, retention)
                }
                ItemKind::Aggregate(a) => {
                    for field in &a.fields {
                        self.field(field, retention);
                    }
                    for item in &a.methods {
                        self.item(item, retention);
                    }
                }
                ItemKind::Enum(e) => {
                    for variant in &e.variants {
                        let retention = self.attachment(
                            &variant.attributes,
                            AttributeTarget::Variant,
                            variant.span,
                            retention,
                        );
                        if let VariantFields::Record(fields) = &variant.fields {
                            for field in fields {
                                self.field(field, retention);
                            }
                        }
                    }
                }
                ItemKind::Trait(t) => {
                    for item in &t.members {
                        self.item(item, retention);
                    }
                }
                ItemKind::Impl(i) => {
                    for item in &i.members {
                        self.item(item, retention);
                    }
                }
                ItemKind::Meta(meta) => match meta {
                    MetaItem::If {
                        then_items,
                        else_items,
                        ..
                    } => {
                        for item in then_items.iter().chain(else_items) {
                            self.defer_generated(item);
                        }
                    }
                    MetaItem::For { items, .. } => {
                        for item in items {
                            self.defer_generated(item);
                        }
                    }
                    MetaItem::Emit(_) => {}
                },
                _ => {}
            }
        }
        self.depth -= 1;
    }
    fn defer_generated(&mut self, item: &Item) {
        self.result.deferred.push(DeferredAttribute {
            attachment: item.span,
            reason: DeferredReason::CompileTimeSelection,
        });
    }
    fn parameters(&mut self, function: &Function, parent: Retention) {
        for param in &function.params {
            self.attachment(
                &param.attributes,
                AttributeTarget::Parameter,
                param.span,
                parent,
            );
        }
    }
    fn field(&mut self, field: &Field, parent: Retention) {
        self.attachment(
            &field.attributes,
            AttributeTarget::Field,
            field.span,
            parent,
        );
    }
    fn attachment(
        &mut self,
        attributes: &[Attribute],
        target: AttributeTarget,
        span: Span,
        parent: Retention,
    ) -> Retention {
        if parent == Retention::Remove {
            return Retention::Remove;
        }
        let mut retention = parent;
        // Determine all cfg clauses first, independently of their source order.
        for attr in attributes
            .iter()
            .filter(|a| a.path.len() == 1 && a.path[0] == "cfg")
        {
            if !self.budget(attr.span) {
                return Retention::Pending;
            }
            if attr.arguments.len() != 1 || attr.string_value.is_some() {
                self.error(
                    "E-PARSE-ATTRIBUTE-ARGUMENTS",
                    attr.span,
                    "cfg requires one configuration expression",
                );
                retention = Retention::Pending;
                continue;
            }
            match config::boolean(&attr.arguments[0], self.context) {
                Some(false) => retention = Retention::Remove,
                None if retention != Retention::Remove => retention = Retention::Pending,
                _ => {}
            }
        }
        match retention {
            Retention::Remove => {
                self.result.removed.push(span);
                return retention;
            }
            Retention::Pending => {
                if !attributes.is_empty() {
                    self.result.deferred.push(DeferredAttribute {
                        attachment: span,
                        reason: DeferredReason::Configuration,
                    });
                }
                return retention;
            }
            Retention::Keep => {}
        }
        let mut seen: BTreeMap<Vec<String>, &Attribute> = BTreeMap::new();
        let mut derived = BTreeSet::new();
        for attribute in attributes {
            if !self.budget(attribute.span) {
                return Retention::Pending;
            }
            let builtin = BUILTIN_ATTRIBUTES
                .iter()
                .find(|spec| attribute.path.len() == 1 && attribute.path[0] == spec.name);
            let custom = self.context.custom.get(&attribute.path);
            let (targets, repeat) = if let Some(spec) = builtin {
                (spec.targets, spec.repeat)
            } else if let Some(spec) = custom {
                (spec.targets.as_slice(), spec.repeat)
            } else {
                self.error(
                    "E-PARSE-ATTRIBUTE-UNKNOWN",
                    attribute.span,
                    format!(
                        "unknown attribute `{}` on a retained attachment",
                        attribute.path.join(".")
                    ),
                );
                continue;
            };
            if !targets.iter().any(|&allowed| {
                allowed == target || (allowed == AttributeTarget::Item && target.is_item())
            }) {
                self.error(
                    "E-PARSE-ATTRIBUTE-TARGET",
                    attribute.span,
                    format!(
                        "attribute `{}` cannot attach to {target:?}",
                        attribute.path.join(".")
                    ),
                );
            }
            if let Some(previous) = seen.get(&attribute.path) {
                if repeat == RepeatRule::Error {
                    let conflict = builtin.is_some_and(|s| s.name == "inline")
                        && (inline_never(previous) || inline_never(attribute));
                    self.error(
                        if conflict {
                            "E-PARSE-ATTRIBUTE-CONFLICT"
                        } else {
                            "E-PARSE-ATTRIBUTE-DUPLICATE"
                        },
                        attribute.span,
                        format!(
                            "attribute `{}` cannot repeat on one attachment",
                            attribute.path.join(".")
                        ),
                    );
                }
            } else {
                seen.insert(attribute.path.clone(), attribute);
            }
            if let Some(spec) = builtin {
                self.arguments(attribute, spec.arguments, &mut derived);
            }
        }
        retention
    }
    fn arguments(
        &mut self,
        attribute: &Attribute,
        rule: ArgumentRule,
        derived: &mut BTreeSet<Vec<String>>,
    ) {
        let count = attribute.arguments.len();
        let equal_string = attribute.string_value.is_some();
        let valid = match rule {
            ArgumentRule::Configuration
            | ArgumentRule::Representation
            | ArgumentRule::Alignment => count == 1 && !equal_string,
            ArgumentRule::OptionalInline => count <= 1 && !equal_string,
            ArgumentRule::String => (count == 1 && !equal_string) || (count == 0 && equal_string),
            ArgumentRule::None => count == 0 && !equal_string,
            ArgumentRule::TraitPaths | ArgumentRule::LintPaths => {
                !equal_string && attribute.arguments.iter().all(|arg| path(arg).is_some())
            }
        };
        if !valid {
            self.error(
                "E-PARSE-ATTRIBUTE-ARGUMENTS",
                attribute.span,
                "attribute argument structure does not match its registered form",
            );
            return;
        }
        if rule == ArgumentRule::TraitPaths {
            for argument in &attribute.arguments {
                let name = path(argument).expect("validated path structure");
                if !derived.insert(name.clone()) {
                    self.error(
                        "E-PARSE-ATTRIBUTE-DUPLICATE-TRAIT",
                        argument.span,
                        format!("derive repeats trait path `{}`", name.join(".")),
                    );
                }
            }
        }
        // Value types, constant evaluation, allowed enum choices, actual trait
        // identities, lint registry membership, and layout limits are later
        // phase obligations. No successful parse means they have been checked.
    }
}
fn inline_never(attribute: &Attribute) -> bool {
    attribute
        .arguments
        .first()
        .and_then(path)
        .is_some_and(|p| p == ["never"])
}
pub(super) fn path(expression: &Expr) -> Option<Vec<String>> {
    match &expression.kind {
        ExprKind::Name(name) => Some(vec![name.clone()]),
        ExprKind::Member {
            object,
            member: MemberName::Name(name),
        } => {
            let mut path = path(object)?;
            path.push(name.clone());
            Some(path)
        }
        _ => None,
    }
}

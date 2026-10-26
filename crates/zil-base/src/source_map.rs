//! Expansion provenance is source data, not a capability or semantic proof.
//! This sidecar leaves source and Core constructors stable while retaining the
//! origin of synthesized nodes, receiver parameters, and generated spans.

use crate::{CompileResult, Diagnostic, FileId, Limits, ModuleId, NodeId, Phase, SourceUnit, Span};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceMapId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceTarget {
    Node(NodeId),
    Parameter { function: NodeId, index: u32 },
    Span(Span),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExpansionRole {
    Source,
    ArrowReturn,
    PipeInput,
    PipeTarget,
    PipeCall,
    RecordBase,
    RecordField,
    RecordDefault,
    RecordResult,
    RecordUpdate,
    ReceiverDefault,
    ReceiverExplicit,
    ReceiverShorthand,
    ScriptMain,
    MetaExpansion,
    SyntaxIsland,
    ClosureBody,
    TemporaryRead,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMapEntry {
    /// Dense entry index. A parent is always a strictly earlier entry.
    pub id: SourceMapId,
    pub target: SourceTarget,
    pub origin: Span,
    pub role: ExpansionRole,
    pub related: Vec<Span>,
    pub parent: Option<SourceMapId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMap {
    pub module_id: ModuleId,
    pub entries: Vec<SourceMapEntry>,
}

fn error(message: impl Into<String>, span: Option<Span>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Expand,
        "E-SOURCE-MAP",
        span,
        message,
    )]
}
fn resource(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(Phase::Expand, message, None)]
}

impl SourceMap {
    pub fn new(module_id: ModuleId) -> Self {
        Self {
            module_id,
            entries: Vec::new(),
        }
    }

    /// Check identities, UTF-8 byte boundaries, existing source files, unique
    /// targets, parent order, and quotas. Target AST existence remains the
    /// producer/consumer's job; this source table cannot certify a Core tree.
    pub fn validate(&self, sources: &[SourceUnit], limits: &Limits) -> CompileResult<()> {
        if self.entries.len() > limits.max_nodes || sources.len() > limits.max_nodes {
            return Err(resource("source-map node budget exceeded"));
        }
        let mut remaining = limits.max_analysis_steps;
        let mut files: BTreeMap<FileId, &SourceUnit> = BTreeMap::new();
        for source in sources {
            remaining = remaining
                .checked_sub(1)
                .ok_or_else(|| resource("source-map validation work budget exceeded"))?;
            if files.insert(source.file_id, source).is_some() {
                return Err(error("duplicate source file identity", Some(source.span())));
            }
        }
        let check_span = |span: Span| -> CompileResult<()> {
            match files.get(&span.file) {
                Some(source) if span.is_valid_for(source) => Ok(()),
                _ => Err(error(
                    "source-map span is absent, outside its source, or not on UTF-8 boundaries",
                    Some(span),
                )),
            }
        };
        let mut targets = BTreeSet::new();
        for (index, entry) in self.entries.iter().enumerate() {
            remaining = remaining
                .checked_sub(entry.related.len().saturating_add(1))
                .ok_or_else(|| resource("source-map validation work budget exceeded"))?;
            self.check_entry(index)?;
            if !targets.insert(&entry.target) {
                return Err(error(
                    "source-map target is mapped more than once",
                    Some(entry.origin),
                ));
            }
            check_span(entry.origin)?;
            for &related in &entry.related {
                check_span(related)?;
            }
            if let SourceTarget::Span(generated) = entry.target {
                check_span(generated)?;
            }
        }
        Ok(())
    }

    /// Return the target-to-root expansion chain. This also fails closed for
    /// malformed raw maps, so callers need not trust a prior `validate` call.
    /// The returned entries remain untrusted provenance, never safety evidence.
    pub fn trace<'a>(
        &'a self,
        target: &SourceTarget,
        limits: &Limits,
    ) -> CompileResult<Vec<&'a SourceMapEntry>> {
        if self.entries.len() > limits.max_nodes || self.entries.len() > limits.max_analysis_steps {
            return Err(resource("source-map lookup budget exceeded"));
        }
        let mut matched = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| &entry.target == target);
        let Some((mut index, _)) = matched.next() else {
            return Ok(Vec::new());
        };
        if matched.next().is_some() {
            return Err(error("source-map target is mapped more than once", None));
        }
        let mut chain = Vec::new();
        let remaining = limits.max_analysis_steps - self.entries.len();
        loop {
            if chain.len() >= limits.max_call_depth || chain.len() >= remaining {
                return Err(resource("source-map expansion depth/work budget exceeded"));
            }
            self.check_entry(index)?;
            let entry = &self.entries[index];
            chain.push(entry);
            match entry.parent {
                Some(parent) => index = parent.0 as usize,
                None => break,
            }
        }
        Ok(chain)
    }

    fn check_entry(&self, index: usize) -> CompileResult<()> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| error("source-map parent is absent", None))?;
        if entry.id.0 as usize != index {
            return Err(error(
                "source-map entry identity is not its index",
                Some(entry.origin),
            ));
        }
        if entry
            .parent
            .is_some_and(|parent| parent.0 as usize >= index)
        {
            return Err(error(
                "source-map parent must precede its child; cyclic or forward parent",
                Some(entry.origin),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> (Vec<SourceUnit>, SourceMap) {
        let sources = vec![
            SourceUnit::new(FileId(0), ModuleId(0), "input.zil", "foo |> read"),
            SourceUnit::new(FileId(1), ModuleId(0), "receiver.zil", "self: &Self"),
        ];
        let map = SourceMap {
            module_id: ModuleId(0),
            entries: vec![
                SourceMapEntry {
                    id: SourceMapId(0),
                    target: SourceTarget::Node(NodeId(1)),
                    origin: Span::new(FileId(0), 0, 11),
                    role: ExpansionRole::PipeCall,
                    related: vec![],
                    parent: None,
                },
                SourceMapEntry {
                    id: SourceMapId(1),
                    target: SourceTarget::Node(NodeId(2)),
                    origin: Span::new(FileId(0), 0, 3),
                    role: ExpansionRole::PipeInput,
                    related: vec![Span::new(FileId(0), 7, 11)],
                    parent: Some(SourceMapId(0)),
                },
                SourceMapEntry {
                    id: SourceMapId(2),
                    target: SourceTarget::Parameter {
                        function: NodeId(3),
                        index: 0,
                    },
                    origin: Span::new(FileId(1), 0, 11),
                    role: ExpansionRole::ReceiverDefault,
                    related: vec![Span::new(FileId(0), 7, 11)],
                    parent: None,
                },
            ],
        };
        (sources, map)
    }
    #[test]
    fn source_map_validates_and_traces_cross_file_provenance() {
        let (sources, map) = sample();
        map.validate(&sources, &Limits::default()).unwrap();
        let trace = map
            .trace(&SourceTarget::Node(NodeId(2)), &Limits::default())
            .unwrap();
        assert_eq!(
            trace.iter().map(|entry| entry.role).collect::<Vec<_>>(),
            vec![ExpansionRole::PipeInput, ExpansionRole::PipeCall]
        );
        assert_eq!(
            map.trace(&SourceTarget::Node(NodeId(99)), &Limits::default())
                .unwrap()
                .len(),
            0
        );
    }
    #[test]
    fn source_map_rejects_cycles_duplicates_and_missing_files() {
        let (sources, mut map) = sample();
        map.entries[0].parent = Some(SourceMapId(1));
        assert!(map.validate(&sources, &Limits::default()).is_err());
        assert!(
            map.trace(&SourceTarget::Node(NodeId(2)), &Limits::default())
                .is_err()
        );
        let (sources, mut map) = sample();
        map.entries[1].target = map.entries[0].target.clone();
        assert!(map.validate(&sources, &Limits::default()).is_err());
        let (mut sources, map) = sample();
        sources.pop();
        assert!(map.validate(&sources, &Limits::default()).is_err());
    }
    #[test]
    fn source_map_checks_utf8_spans_and_depth_budget() {
        let (mut sources, mut map) = sample();
        sources[0].text = "语言abcdefg".into();
        map.entries[0].origin = Span::new(FileId(0), 1, 3);
        assert!(map.validate(&sources, &Limits::default()).is_err());
        let (_, map) = sample();
        let limits = Limits {
            max_call_depth: 1,
            ..Limits::default()
        };
        let error = map
            .trace(&SourceTarget::Node(NodeId(2)), &limits)
            .unwrap_err();
        assert_eq!(error[0].category, crate::ResultCategory::ResourceLimit);
    }
}

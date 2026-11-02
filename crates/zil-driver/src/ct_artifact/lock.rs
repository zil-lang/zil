//! A deliberately small, strict text representation shared by file and memory
//! requests. The compiler never writes a lock or repairs a stale one.
use std::collections::{BTreeMap, BTreeSet};
use zil_base::{
    CompileResult, Diagnostic, Fingerprint, ModuleId, NodeId, Phase, RULE_VERSION, SourceUnit, Span,
};
use zil_ir::ct_artifact::{
    CT_ARTIFACT_SCHEMA_VERSION, CtDelimiter, CtLockedProviderRegistry, CtProviderEntry, CtSymbolId,
    CtSyntaxCategory,
};
use zil_syntax::ast::{ItemKind, Module};

const HEADER: &str = "zil-syntax-lock 1";
const GRAMMAR_DOMAIN: &[u8] = b"zil/provider-grammar/v1\0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxLock {
    pub entries: Vec<SyntaxLockEntry>,
    pub fingerprint: Fingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxLockEntry {
    pub path: Vec<String>,
    pub export: String,
    pub function: String,
    pub output: CtSyntaxCategory,
    pub delimiters: Vec<CtDelimiter>,
    pub source_fingerprint: Fingerprint,
    pub grammar_fingerprint: Fingerprint,
}

fn error(code: &str, span: Option<Span>, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Expand, code, span, message)]
}

fn word(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn category(text: &str) -> Option<CtSyntaxCategory> {
    Some(match text {
        "Expr" => CtSyntaxCategory::Expr,
        "Type" => CtSyntaxCategory::Type,
        "StmtList" => CtSyntaxCategory::StmtList,
        "ItemList" => CtSyntaxCategory::ItemList,
        _ => return None,
    })
}

fn delimiters(text: &str) -> Option<Vec<CtDelimiter>> {
    let mut result = Vec::new();
    for part in text.split(',') {
        let value = match part {
            "paren" => CtDelimiter::Parenthesis,
            "bracket" => CtDelimiter::Bracket,
            "brace" => CtDelimiter::Brace,
            _ => return None,
        };
        if result.contains(&value) {
            return None;
        }
        result.push(value);
    }
    (!result.is_empty()).then_some(result)
}

/// Grammar identity binds the exact source implementation to this version of
/// the fixed provider token contract. An external lock writer can compute it
/// from the source hash without executing a provider.
pub fn grammar_fingerprint(source: Fingerprint) -> Fingerprint {
    let mut bytes = GRAMMAR_DOMAIN.to_vec();
    bytes.extend_from_slice(RULE_VERSION.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&source.0);
    Fingerprint::of(&bytes)
}

impl SyntaxLock {
    pub fn parse(text: &str) -> CompileResult<Self> {
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            return Err(error(
                "E-SYNTAX-LOCK-FORMAT",
                None,
                "syntax lock header must be `zil-syntax-lock 1`",
            ));
        }
        let rule = format!("rule {RULE_VERSION}");
        if lines.next() != Some(rule.as_str()) {
            return Err(error(
                "E-SYNTAX-LOCK-RULE",
                None,
                "syntax lock CT rule version is stale or missing",
            ));
        }
        let mut entries = Vec::new();
        let mut keys = BTreeSet::new();
        for (offset, line) in lines.enumerate() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let words = line.split_ascii_whitespace().collect::<Vec<_>>();
            if words.len() != 8 || words[0] != "provider" {
                return Err(error(
                    "E-SYNTAX-LOCK-FORMAT",
                    None,
                    format!("syntax lock line {} must have eight fields", offset + 3),
                ));
            }
            let path = words[1].split('.').map(str::to_owned).collect::<Vec<_>>();
            if path.is_empty()
                || !path.iter().all(|s| word(s))
                || !word(words[2])
                || !word(words[3])
            {
                return Err(error(
                    "E-SYNTAX-LOCK-FORMAT",
                    None,
                    format!("invalid path, export, or function on line {}", offset + 3),
                ));
            }
            let Some(output) = category(words[4]) else {
                return Err(error(
                    "E-SYNTAX-LOCK-CATEGORY",
                    None,
                    format!("invalid output category on line {}", offset + 3),
                ));
            };
            let Some(delimiters) = delimiters(words[5]) else {
                return Err(error(
                    "E-SYNTAX-LOCK-DELIMITER",
                    None,
                    format!("invalid delimiter list on line {}", offset + 3),
                ));
            };
            let Some(source_fingerprint) = Fingerprint::from_hex(words[6]) else {
                return Err(error(
                    "E-SYNTAX-LOCK-FORMAT",
                    None,
                    format!("invalid source hash on line {}", offset + 3),
                ));
            };
            let Some(grammar_fingerprint) = Fingerprint::from_hex(words[7]) else {
                return Err(error(
                    "E-SYNTAX-LOCK-FORMAT",
                    None,
                    format!("invalid grammar hash on line {}", offset + 3),
                ));
            };
            if grammar_fingerprint != self::grammar_fingerprint(source_fingerprint) {
                return Err(error(
                    "E-SYNTAX-LOCK-GRAMMAR",
                    None,
                    format!("stale grammar hash on line {}", offset + 3),
                ));
            }
            if !keys.insert((path.clone(), words[2].to_owned())) {
                return Err(error(
                    "E-SYNTAX-LOCK-DUPLICATE",
                    None,
                    format!("duplicate provider export on line {}", offset + 3),
                ));
            }
            entries.push(SyntaxLockEntry {
                path,
                export: words[2].to_owned(),
                function: words[3].to_owned(),
                output,
                delimiters,
                source_fingerprint,
                grammar_fingerprint,
            });
        }
        if entries.is_empty() {
            return Err(error(
                "E-SYNTAX-LOCK-EMPTY",
                None,
                "syntax lock has no providers",
            ));
        }
        Ok(Self {
            entries,
            fingerprint: Fingerprint::of(text.as_bytes()),
        })
    }

    /// Recheck every entry against the exact request graph, including entries
    /// not selected by a particular alias. A stale lock cannot hide behind an
    /// unused export and become active on a warm-cache request.
    pub fn resolve(
        &self,
        sources: &[SourceUnit],
        syntax: &[&Module],
        paths: &BTreeMap<Vec<String>, ModuleId>,
    ) -> CompileResult<CtLockedProviderRegistry> {
        if sources.len() != syntax.len() {
            return Err(error(
                "E-SYNTAX-LOCK-INPUT",
                None,
                "source and syntax module counts differ",
            ));
        }
        let modules = sources
            .iter()
            .zip(syntax)
            .map(|(source, module)| (source.module_id, (source, *module)))
            .collect::<BTreeMap<_, _>>();
        let mut resolved = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            let path = entry.path.join(".");
            let Some(module) = paths.get(&entry.path).copied() else {
                return Err(error(
                    "E-SYNTAX-LOCK-MODULE",
                    None,
                    format!("locked provider module {path} is not in the request graph"),
                ));
            };
            let Some((source, parsed)) = modules.get(&module) else {
                return Err(error(
                    "E-SYNTAX-LOCK-MODULE",
                    None,
                    format!("locked provider module {path} has no source"),
                ));
            };
            if source.fingerprint() != entry.source_fingerprint {
                return Err(error(
                    "E-SYNTAX-LOCK-STALE",
                    None,
                    format!("source for locked provider {path} changed"),
                ));
            }
            let matches = parsed
                .items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| match &item.kind {
                    ItemKind::Function(function)
                        if item.public && function.name == entry.function =>
                    {
                        Some((index, item.span))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(index, _span)] = matches.as_slice() else {
                return Err(error(
                    "E-SYNTAX-LOCK-ENTRY",
                    None,
                    format!(
                        "locked function {} in {path} must name exactly one public function",
                        entry.function
                    ),
                ));
            };
            let declaration = NodeId(u32::try_from(*index).map_err(|_| {
                error(
                    "E-SYNTAX-LOCK-LIMIT",
                    None,
                    "provider declaration index overflow",
                )
            })?);
            resolved.push(CtProviderEntry {
                path,
                export: entry.export.clone(),
                symbol: CtSymbolId {
                    module,
                    declaration,
                    generation_path: None,
                },
                output: entry.output,
                delimiters: entry.delimiters.clone(),
                implementation_fingerprint: entry.source_fingerprint,
                grammar_fingerprint: entry.grammar_fingerprint,
            });
        }
        Ok(CtLockedProviderRegistry {
            schema_version: CT_ARTIFACT_SCHEMA_VERSION,
            entries: resolved,
            fingerprint: self.fingerprint,
        })
    }
}

pub(super) fn provider_entry<'a>(
    registry: &'a CtLockedProviderRegistry,
    path: &str,
    export: &str,
    delimiter: CtDelimiter,
    span: Span,
) -> CompileResult<&'a CtProviderEntry> {
    let entry = registry
        .entries
        .iter()
        .find(|entry| entry.path == path && entry.export == export)
        .ok_or_else(|| {
            error(
                "E-SYNTAX-LOCK-ENTRY",
                Some(span),
                format!("provider {path} has no locked export {export}"),
            )
        })?;
    if !entry.delimiters.contains(&delimiter) {
        return Err(error(
            "E-SYNTAX-LOCK-DELIMITER",
            Some(span),
            format!("provider {path}.{export} does not allow this delimiter"),
        ));
    }
    Ok(entry)
}

//! Explicit closed-executable policy, applied only after the original O2 run.
//! This is not source-private visibility or a language verification capability.
use crate::model::{Item, Linkage, Module};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LateClosedPlan {
    module: Module,
    names: Vec<String>,
}
impl LateClosedPlan {
    /// The caller must establish the explicit no-external-ZIL-exports contract
    /// and complete pre-O2 cross-unit/address-user closure. The plan binds that
    /// selected set to exact actual machine inputs, never a diagnostic digest.
    pub fn new(module: &Module, names: impl IntoIterator<Item = String>) -> Self {
        let defined = module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(f) if f.blocks.is_some() && f.linkage == Linkage::External => {
                    Some(f.name.as_str())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let names = names
            .into_iter()
            .filter(|n| defined.contains(n.as_str()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Self {
            module: module.clone(),
            names,
        }
    }
    pub fn candidates(&self) -> &[String] {
        &self.names
    }
    #[cfg(any(feature = "native", test))]
    pub(crate) fn matches(&self, module: &Module) -> bool {
        self.module.triple == module.triple && self.module.items == module.items
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LateClosedReport {
    pub candidates: Vec<String>,
    pub eligible: Vec<String>,
    pub before_definitions: Vec<String>,
    pub after_definitions: Vec<String>,
    pub removed: Vec<String>,
    pub refused: BTreeMap<String, usize>,
    pub cost_ns: u64,
    pub ran_globaldce: bool,
}
impl LateClosedReport {
    pub fn to_json(&self) -> String {
        fn quoted(s: &str) -> String {
            use std::fmt::Write;
            let mut out = String::from("\"");
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if c < ' ' => {
                        write!(&mut out, "\\u{:04x}", u32::from(c)).unwrap();
                    }
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
        let array = |v: &[String]| {
            format!(
                "[{}]",
                v.iter().map(|s| quoted(s)).collect::<Vec<_>>().join(",")
            )
        };
        let refused = self
            .refused
            .iter()
            .map(|(k, v)| format!("{}:{v}", quoted(k)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"policy\":\"late-closed-v1\",\"candidates\":{},\"eligible\":{},\"before_definitions\":{},\"after_definitions\":{},\"removed\":{},\"refused\":{{{}}},\"cost_ns\":{},\"ran_globaldce\":{}}}",
            array(&self.candidates),
            array(&self.eligible),
            array(&self.before_definitions),
            array(&self.after_definitions),
            array(&self.removed),
            refused,
            self.cost_ns,
            self.ran_globaldce
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_is_bound_to_actual_machine_items_not_fingerprint() {
        let mut m = Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "same".into(),
            items: vec![].into(),
        };
        let p = LateClosedPlan::new(&m, Vec::new());
        m.fingerprint = "new diagnostic".into();
        assert!(p.matches(&m));
        m.triple = "other".into();
        assert!(!p.matches(&m));
        let report = LateClosedReport {
            removed: vec!["quote\"\n".into()],
            ..LateClosedReport::default()
        };
        assert!(report.to_json().contains("quote\\\"\\n"));
    }
}

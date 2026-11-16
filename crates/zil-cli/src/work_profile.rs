//! Executable policy only; the SDK and base limits retain their defaults.
use zil_base::{Limits, json_string};
use zil_driver::CompileReport;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum WorkProfile {
    Standard,
    #[default]
    Alpha,
}

impl WorkProfile {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "standard" => Some(Self::Standard),
            "alpha" => Some(Self::Alpha),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Alpha => "alpha",
        }
    }

    fn revision(self) -> &'static str {
        match self {
            Self::Standard => "standard-10m-v1",
            Self::Alpha => "alpha-64m-v2",
        }
    }

    pub(super) fn apply(self, limits: &mut Limits) {
        limits.max_analysis_steps = match self {
            Self::Standard => 10_000_000,
            Self::Alpha => 64_000_000,
        };
    }

    pub(super) fn report_json(self, report: &CompileReport, limits: &Limits) -> String {
        self.annotate_json(&report.to_json(), limits)
    }

    pub(super) fn annotate_json(self, object: &str, limits: &Limits) -> String {
        // Keep the driver wire format intact; append executable policy metadata.
        let prefix = object.strip_suffix('}').expect("internal JSON object");
        format!(
            "{prefix},\"work_profile\":{},\"work_profile_revision\":{},\"effective_limits\":{}}}",
            json_string(self.name()),
            json_string(self.revision()),
            limits_json(limits),
        )
    }
}

fn limits_json(limits: &Limits) -> String {
    // Exhaustive destructuring requires new Limits fields to be reported here.
    let Limits {
        max_source_bytes,
        max_tokens,
        max_nodes,
        max_functions,
        max_blocks,
        max_instructions,
        max_locals,
        max_values,
        max_analysis_steps,
        max_ct_steps,
        max_call_depth,
    } = limits;
    format!(
        "{{\"max_source_bytes\":{max_source_bytes},\"max_tokens\":{max_tokens},\"max_nodes\":{max_nodes},\"max_functions\":{max_functions},\"max_blocks\":{max_blocks},\"max_instructions\":{max_instructions},\"max_locals\":{max_locals},\"max_values\":{max_values},\"max_analysis_steps\":{max_analysis_steps},\"max_ct_steps\":{max_ct_steps},\"max_call_depth\":{max_call_depth}}}"
    )
}

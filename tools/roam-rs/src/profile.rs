//! Tool-local profile routing. This is not a compiler IR or language extension.
use crate::{
    byte_buffer, callable_composition, domain, native, protocol, resource_closure, scalar, search,
    task_results,
};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Session,
    Scalar,
    ResourceClosure,
    ByteBuffer,
    CallableComposition,
    TaskResults,
}
impl Profile {
    pub fn is_native(self) -> bool {
        self != Self::Session
    }
    pub fn named(name: &str) -> Result<Self, String> {
        match name {
            "session" => Ok(Self::Session),
            "scalar-native" => Ok(Self::Scalar),
            "resource-closure-native" => Ok(Self::ResourceClosure),
            "byte-buffer-native" => Ok(Self::ByteBuffer),
            "task-results-native" => Ok(Self::TaskResults),
            "callable-composition-native" => Ok(Self::CallableComposition),
            _ => Err("unknown quality profile".into()),
        }
    }
    pub fn for_case(trace: &Value) -> Result<Self, String> {
        match trace["contract"].as_str() {
            Some(protocol::CONTRACT) => Ok(Self::Session),
            Some(task_results::CONTRACT) => Ok(Self::TaskResults),
            Some(callable_composition::CONTRACT) => Ok(Self::CallableComposition),
            Some(scalar::CONTRACT) => Ok(Self::Scalar),
            Some(resource_closure::CONTRACT) => Ok(Self::ResourceClosure),
            Some(byte_buffer::CONTRACT | byte_buffer::owned::CONTRACT) => Ok(Self::ByteBuffer),
            _ => Err("unsupported quality contract".into()),
        }
    }
    pub fn contract(self) -> &'static str {
        match self {
            Self::Session => protocol::CONTRACT,
            Self::Scalar => scalar::CONTRACT,
            Self::ResourceClosure => resource_closure::CONTRACT,
            Self::ByteBuffer => byte_buffer::owned::CONTRACT,
            Self::CallableComposition => callable_composition::CONTRACT,
            Self::TaskResults => task_results::CONTRACT,
        }
    }
    pub fn default_steps(self) -> usize {
        match self {
            Self::Session => 64,
            Self::Scalar | Self::ResourceClosure => 4,
            Self::ByteBuffer | Self::CallableComposition | Self::TaskResults => 2,
        }
    }
    pub fn max_steps(self) -> usize {
        match self {
            Self::Session => 249,
            Self::Scalar => scalar::MAX_PROGRAMS,
            Self::ResourceClosure => resource_closure::MAX_PROGRAMS,
            Self::ByteBuffer => byte_buffer::MAX_PROGRAMS,
            Self::CallableComposition => callable_composition::MAX_PROGRAMS,
            Self::TaskResults => task_results::MAX_PROGRAMS,
        }
    }
    pub fn generate(self, seed: i64, steps: usize) -> Value {
        match self {
            Self::Session => search::generate(seed, steps),
            Self::Scalar => scalar::generate(seed, steps),
            Self::ResourceClosure => resource_closure::generate(seed, steps),
            Self::ByteBuffer => byte_buffer::owned::generate(seed, steps),
            Self::CallableComposition => callable_composition::generate(seed, steps),
            Self::TaskResults => task_results::generate(seed, steps),
        }
    }
    pub fn fixed(self) -> Vec<(String, Value)> {
        match self {
            Self::Session => search::fixed(),
            Self::Scalar => scalar::fixed(),
            Self::ResourceClosure => resource_closure::fixed(),
            Self::CallableComposition => callable_composition::fixed(),
            Self::TaskResults => task_results::fixed(),
            Self::ByteBuffer => byte_buffer::fixed()
                .into_iter()
                .chain(byte_buffer::owned::fixed())
                .collect(),
        }
    }
    pub fn fault_name(self) -> &'static str {
        match self {
            Self::Session => "stale-after-edit",
            Self::Scalar => "scalar-sub-to-add",
            Self::ResourceClosure => "closure-omit-call",
            Self::ByteBuffer => "bytes-omit-push",
            Self::CallableComposition => "callable-change-command",
            Self::TaskResults => "task-change-result",
        }
    }
    pub fn fault(self, name: &str) -> Result<bool, String> {
        if name == "none" {
            Ok(false)
        } else if name == self.fault_name() {
            Ok(true)
        } else {
            Err("controlled mutation does not belong to this profile".into())
        }
    }
}

pub fn execute(
    trace: &Value,
    options: &domain::Options,
    fault_name: &str,
    before: impl FnMut(usize, &Value) -> Result<(), String>,
    after: impl FnMut(usize) -> Result<(), String>,
) -> Value {
    if let Err(e) = protocol::validate_case(trace) {
        return protocol::issue("invalid", 0, "case", -1, "InvalidCase", &e);
    }
    let profile = match Profile::for_case(trace) {
        Ok(v) => v,
        Err(e) => return protocol::issue("unsupported", 0, "init", -1, "Unsupported", &e),
    };
    if let Err(e) = profile.fault(fault_name) {
        return protocol::issue("invalid", 0, "options", -1, "InvalidMutation", &e);
    }
    match profile {
        Profile::Session => domain::execute(trace, options, before, after),
        Profile::Scalar
        | Profile::ResourceClosure
        | Profile::ByteBuffer
        | Profile::CallableComposition
        | Profile::TaskResults => native::execute(trace, options, before, after),
    }
}

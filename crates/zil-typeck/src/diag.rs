//! Diagnostic codes produced by typeck. One list, so registration in
//! `spec/diagnostics.json` has a single source.

use zil_base::{Diagnostic, Phase, Span};

pub const NAME_UNRESOLVED: &str = "E-NAME-UNRESOLVED";
pub const NAME_DUPLICATE: &str = "E-NAME-DUPLICATE";
pub const TYPE_MISMATCH: &str = "E-TYPE-MISMATCH";
pub const TYPE_INFERENCE: &str = "E-TYPE-INFERENCE";
pub const TYPE_ARGUMENT: &str = "E-TYPE-ARGUMENT";
pub const GENERIC_BOUND: &str = "E-GENERIC-ARGUMENT";
pub const CALL_ARITY: &str = "E-CALL-ARITY";
pub const LITERAL_RANGE: &str = "E-LITERAL-RANGE";
pub const FLOAT_LITERAL: &str = "E-FLOAT-LITERAL";
pub const NUMERIC_CONVERSION: &str = "E-NUMERIC-CONVERSION";
pub const OPERATOR_TYPE: &str = "E-OPERATOR-TYPE";
pub const UNKNOWN_FIELD: &str = "E-UNKNOWN-FIELD";
pub const RECORD_MISSING_FIELD: &str = "E-RECORD-MISSING-FIELD";
pub const RECORD_DUPLICATE_FIELD: &str = "E-RECORD-DUPLICATE-FIELD";
pub const RECORD_UNKNOWN_FIELD: &str = "E-RECORD-UNKNOWN-FIELD";
pub const ENUM_VARIANT: &str = "E-ENUM-VARIANT";
pub const ENUM_ARITY: &str = "E-ENUM-ARITY";
pub const MATCH_ARITY: &str = "E-MATCH-ARITY";
pub const MATCH_NONEXHAUSTIVE: &str = "E-MATCH-NONEXHAUSTIVE";
pub const PATTERN_REFUTABLE: &str = "E-PATTERN-REFUTABLE";
pub const ASSIGN_IMMUTABLE: &str = "E-ASSIGN-IMMUTABLE";
pub const ASSIGN_TARGET: &str = "E-ASSIGN-TARGET";
pub const MISSING_RETURN: &str = "E-MISSING-RETURN";
pub const LOOP_CONTROL: &str = "E-LOOP-CONTROL";
pub const ARRAY_LENGTH: &str = "E-ARRAY-LENGTH";
pub const FFI_DECLARATION: &str = "E-FFI-DECLARATION";
pub const FFI_UNSAFE: &str = "E-FFI-UNSAFE";
pub const DROP_BINDING: &str = "E-DROP-BINDING";
pub const DROP_DUPLICATE: &str = "E-DROP-DUPLICATE";
pub const DROP_SIGNATURE: &str = "E-DROP-SIGNATURE";
pub const RESOURCE_TYPE: &str = "E-RESOURCE-TYPE";
pub const RESOURCE_RESULT_MODE: &str = "E-RESOURCE-RESULT-MODE";
pub const GIVE_DESCRIPTOR: &str = "E-GIVE-DESCRIPTOR";
pub const GIVE_TYPE: &str = "E-GIVE-TYPE";
pub const ERROR_INJECTION: &str = "E-ERROR-INJECTION";
pub const ERROR_INJECTION_AMBIGUOUS: &str = "E-ERROR-INJECTION-AMBIGUOUS";
pub const TRY_NOT_ERROR: &str = "E-TRY-NOT-ERROR";
pub const TRY_RETURN_TYPE: &str = "E-TRY-RETURN-TYPE";
pub const TRY_ERROR_TYPE: &str = "E-TRY-ERROR-TYPE";

pub fn error(code: &str, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(Phase::Types, code, Some(span), message)
}

pub fn unsupported(feature: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, feature, Some(span))
}

pub fn limit(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::limit(Phase::Types, message, Some(span))
}

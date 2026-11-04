Artifact Sum follow-on to clean f01e61bcc (not acceptance evidence)

CT artifact/envelope version3 rejects all previous v2 bytes. Fingerprint domains
for program/provider/dependencies and standalone value/function payloads move to
v3; public nominal schema bytes now include complete actual ordered arguments.
CtSchemaArgument::{Type(CtTy),Scalar(Constant)} preserves phantom and constant
arguments. Independent admission checks argument type/count/kind against the
registered declaration/actual instance and rejects cyclic identity dependencies;
schema IDs and equal field layouts never stand in for nominal identity.

CtTy::AnonymousSum(Vec<CtTy>) wire tag16 is a structural pure CT sum descriptor.
CtValue::AnonymousSum{members,active,payload} wire tag16 retains all inactive
member identities. Members must be flattened, strictly canonical, at least two,
and distinct by full resolved constructor/nominal symbol+ordered argument
identity; no hashes or numeric schema-ID sorting. Active must be in range and
payload must match its exact member type. Raw descriptors and values grant no
symbol, reflected shape, syntax-origin, Copy or runtime access capability.

CtInstructionKind::MakeAnonymousSum{members,active,payload} wire opcode37 performs
exact injection after independent descriptor validation. Existing VariantTag
and VariantField consume this structural one-field sum as well as nominal
Enums; field must be zero and VM extraction must match the real active tag.
Source inference never tries numeric conversions across members. Constructor
identities may use a different canonical order from runtime Type: every bridge
renormalizes complete runtime member Types and maps the active member by that
Type, never by carrying an internal index across phases.

All type/argument graph traversal, full-key copies/comparisons, normalization,
Wire stacks and value allocation use current mutable request ledgers before
copy/allocation. New recursive Wire Type/Value forms remain iterative; the new
Value continuation descriptor is boxed only inside the compile-time bounded
codec to preserve the previous inline continuation frame size. This adds no
runtime ZIL box or permission. v2 rejection, source/authentication, complete
phantom/const lineage, graph cycle/expansion limits, nearest raw/Wire controls
and warm/fresh N/N-1 remain mandatory evidence for the subsequent value stage.

# Preserve machine text bytes with direct String rendering

Stage14 baseline 5821120ac5952bcf799afe76de1030741d9ccb89. The complete LLVM
sidecar remains a required diagnostic/reference artifact. Current Module::to_text
formats every type, operand and argument through nested std::fmt dispatch; the
opaque branch output is about 3 MB and its measured rendering phase is about
23 ms. This proposal changes only rendering mechanics, not whether rendering
occurs or any machine transformation.

Use private String append helpers for types, values, arguments and instruction
syntax. Append existing names and fixed punctuation directly; use standard
numeric formatting for arbitrary integer widths, array lengths, indices,
alignment and i128 literals. Preserve the public Display implementations and
Module::to_text API. Avoid temporary named-type field clones while preserving the
existing special named empty-struct spelling `{  }`, distinct from ordinary `{}`.

The exact bytes must remain unchanged for every model shape, including malformed
names/values/types that are still printable, all linkage/ABI attributes, variadic
calls, nested constant values, all byte escapes and signed integer extremes.
Retain the old formatter under cfg(test) as an independent byte oracle with a
compact all-variant differential fixture. Existing rendered/direct native tests
remain appropriate integration controls; root alone builds/tests/measures. The
complete current Verify, lowering, machine preflight, object-cache identity and
sidecar publication paths remain unchanged. No new cache, dependency, unsafe code,
public schema or language budget change. A whole-request gain is unproven until
the selected measurement window.

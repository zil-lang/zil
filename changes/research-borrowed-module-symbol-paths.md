# Borrow module symbol lookup paths during expansion

Stage12 baseline f15cf54a4. The project normalization visitor constructs a temporary
`Vec<String>` for every expression name and simple module member lookup. These
copies do not belong to the resulting syntax. In Restore mode, the subsequent
symbol lookup always returns None, so the copied path is unused.

Accept borrowed string elements in the existing symbol lookup. Use the original
name slice or a two-element stack array for expression lookup, and omit that
lookup in Restore mode. Keep every existing node/text charge, lexical-shadowing
check, Self rewrite, privacy diagnostic, canonical-name allocation, restoration,
generated import and recursive visit in its existing order. No syntax subtree,
current validation, specialization step or publication artifact is omitted.

Reuse existing multi-module source, alias/binder, private-export and budget
controls. If desired, a private diagnostic compiler can count name/member
lookups by visitor mode and the temporary string bytes; these counts establish
removed allocations, not elapsed-time improvement. Complete-request timing must
determine the actual benefit. This change does not assume the previous scalar
shape borrowing experiment produced a net improvement.

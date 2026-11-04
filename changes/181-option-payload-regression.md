# Preserve the existing owning Option payload profile

Task-owning integration0bccd16e8 removed the sealed Option exception while
extending generic enum payload checks for explicit Managed values. An already
supported `Option<Package>` consequently entered the legacy Copy classifier and
rejected. Restore the Option exception present in accepted main4bc1d77b5; keep
the ordinary generic enum and Task Poll checks. Types, Analyze and Verify still
check actual payload layout and responsibility, with no fabricated Copy right.

The existing aggregates target exposed eight failures in owning Option and
reference-pattern/native controls. After restoring the exception, all50
terminal340 tests pass on the composed40 candidate, including budget neighbors,
raw permissions and both native backends. This restores admitted behavior;
broader composed-tree checks and independent review are recorded separately.

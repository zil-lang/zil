# 0121 — Instantiate nominal types inside callable bounds

Status: authorized implementation slice 121; independent acceptance pending.

## Problem and finite change

The recovered draft22 specialization engine visits ordinary function parameter and
result types, but leaves the argument/result types nested inside a retained
`F: Fn(...) -> ...`, `FnMut`, or `FnOnce` bound unchanged. A generic nominal such as
an imported `Cell<i64>` can therefore have a generated nominal identity in the
ordinary parameter while retaining its unspecialized application in the callable
contract. The same valid type then fails downstream checking.

Before visiting ordinary function parameters, traverse only the nested type
arguments of the already validated direct callable bound through the existing
bounded `Engine::ty` path. Keep the outer callable keyword intact. The existing
instance table, template identity, alias normalization and module binding remain
the sole source of generated nominal identities. No new capability or proof is
created by expansion; Types, Analyze and independent Verify still check the result.

## Acceptance and budget impact

This repairs a finite false rejection: concrete generic record/enum arguments and
results can cross the existing synchronous callable-helper boundary, including
transparent imported aliases. It does not add escaping callable parameters,
recursive callable signatures, additional generic bounds, or general traits.
Wrong nominal owners and wrong specializations remain errors.

The newly traversed bound and nested types are actual work and are charged using
the existing node/work/depth budget. Default limits do not change. A program that
previously consumed an undercounted budget can now reach ResourceLimit earlier;
the old exact quota boundary is not claimed equivalent. The corrected result must
be complete at its new N boundary and rejected at N−1. No partial publication is
accepted when work runs out.

## Validation

Preserve the original recovered-baseline failure. Exercise actual cross-module
record and enum values through Fn, FnMut and owning FnOnce at O0/O2, with observable
mutable state and consuming owned captures. Include different-module nominal and
different-specialization counterexamples; retain the existing unsupported callable
boundaries. Check exact expansion work/node/depth boundaries and independent
verifier rejection of a changed callable parameter type. Existing helper and alias
regression suites provide nearby compatibility checks.

Schema/wire17, production gates, runtime/ABI layout, default limits and normative
files remain unchanged. This is a new reconstruction from the recovered source,
not recovery of the missing original 116 commit or inheritance of its old tests.

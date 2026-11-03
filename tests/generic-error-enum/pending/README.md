# Remaining generic error-union contexts

These are legal intended programs, still rejected at Expand on the reviewed
`369bd222` implementation. They are preserved follow-up inputs, not passing
negative tests. No native execution is claimed.

- `unique-binding-value.zil`: an explicit `Choice<i32>!i32` binding should supply
  the unique enum context and inject `Choice.None`; the generic binding checker
  still requires strict same-type initialization.
- `unique-call-argument.zil`: the concrete sink parameter supplies the same unique
  context; generic argument matching does not yet apply the injection rule.

The accepted return fix is separate. A successor should retain these exact
sources and ordinary-function controls, preserve both-side ambiguity and full
nominal/type-argument rejection, and check actual source-to-machine behavior.

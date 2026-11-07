# Reserved trait names after module rewriting

Module-generated names must not let a user declaration impersonate built-in `Send`, `Sync`, `Drop` or `Fn` family traits. For a genuine `$module<decimal>_` prefix, the nominal bound/declaration check recognizes the original name solely to apply the built-in reserved-name exclusion. It does not shorten nominal identity, accept aliases or reject ordinary near names such as `User_Send`.

The check applies to inline and `where` bounds, real module rewrites, forged prefixes and unused definitions. It uses the existing generic trait bound path and charges each scan against the current request. The governing rule is `_zil_design.md` §18.

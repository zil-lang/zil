# Possible reuse of a native link discovery plan

The implemented Linux path executes the current clang `-###`, tool dependency
probes and GNU ld `--verbose` before deriving the final-link key. It binds
current object bytes, tool identities, environment, library search inventories
and supported recursive library/script inputs. Reusing the final executable
therefore still has discovery costs. No speed benefit is claimed for replacing a
short native link until the release driver profile separates those costs.

This note describes a possible later optimization. It adds no implementation,
permission, verification capability or default option. In particular, a saved
probe output alone cannot authorize omitting a current probe.

## Dependencies that a saved plan would have to cover

A plan would retain the parsed actual linker argument vector, with only the
explicit current object and staging-output argument slots replaced by indexed
placeholders. Its request identity must bind the original clang argv0 spelling,
resolved executable and tool identity, target, optimization, thread option,
ordered objects, current working directory and complete environment. A changed
object remains a current input even when the external discovery plan is equal.

The complete discovery dependency set is broader than the `-L` directories and
files printed by the previous command:

* Clang can load default driver configuration files from its user, system and
  executable directories. Previously absent configuration files can become
  inputs without changing the clang binary or any previous link library.
* GCC installation detection searches candidate prefixes, target aliases,
  versions, multilib qualification files and Gentoo configuration. Installing a
  newer candidate, or completing a previously invalid candidate, can change
  startup objects and library resolution while all previously selected files
  remain unchanged.
* Linker selection searches program paths and PATH candidates. A new earlier
  executable or a changed symlink/permission must invalidate the selection.
* The dynamic loader resolves tool dependencies using its cache, preload state,
  configured paths and hardware capability candidates. Hashing only the DSOs
  selected previously does not establish that they would still be selected.
* GNU linker built-in scripts, all supported library search roots, absence of
  roots, all same-name `.so` and `.a` candidates, relative script-member
  candidates, scripts and their recursive inputs remain dependencies. A saved
  first-existing candidate is insufficient because ld can skip incompatible
  candidates.

A later implementation needs a bounded, explicit proof for every supported
lookup rule, including negative lookups and newly created directories/files.
Unknown driver wrappers, configuration syntax, library formats, lookup roots or
budget exhaustion must continue through current discovery and linking. File
metadata alone is not a new content-assurance policy; changing the existing
trusted-installed-tool model or library content checks requires separate review.
A recipe must remain raw session data and cannot replace current independent
Verify, current machine preflight, cancellation or publication guards.

## Fixed source references

The installed clang reports LLVM commit
`7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`. These source references are fixed to
that commit, rather than a moving LLVM branch:

* [Driver configuration initialization and loading](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/clang/lib/Driver/Driver.cpp#L207):
  `loadConfigFiles` and `loadDefaultConfigFiles` describe user/system/executable
  search directories and configuration names; verbose diagnostics report the
  configured directories.
* [GCC installation discovery](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/clang/lib/Driver/ToolChains/Gnu.cpp#L2107):
  `GCCInstallationDetector::init`, `AddDefaultGCCPrefixes`,
  `ScanLibDirForGCCTriple` and `ScanGentooConfigs` enumerate additional inputs.
* [Linker program selection](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/clang/lib/Driver/Driver.cpp#L6880):
  `GetProgramPath` searches program paths and PATH; the related
  [toolchain selection code](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/clang/lib/Driver/ToolChain.cpp#L1271)
  handles linker names and overrides.

A local read-only syscall trace was useful to discover negative loader and
program-path lookups, but it is not a proven production dependency receipt.
It does not prove the absence of clock, ambient wrapper or other unmodeled
inputs. No strace dependency or generic syscall-replay mechanism is proposed for
this implementation. Further work starts with the actual release profile and a
small independently reviewed platform dependency model, if justified.

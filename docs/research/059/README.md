# 059 — complete finite resource relations and event cuts

Research workspace only; no production hot path, acceptance rule, schema or gate
is changed. Production TypedIR is frozen at schema 8 / source commit
`6acda08c72dbd3e387893b4260770f0834dcdbe0`. Schema 9 mainline work is separate.

**Measured decision: current candidate NoGo for the production core.** Complete
finite joint relations agree with the independent oracle, but the real example is
slower and the four-guard resource case exhausts the BDD-node budget.

* [RESULTS](RESULTS.md): frozen identities, complete-cost measurements, precision
  witness, excluded bad shapes, limitations and explicit next admission conditions.
* [CONTRACT](CONTRACT.md): admitted abstraction, unsupported multiplicity, raw
  event labels, conservative Desc future use, full-cost and budget boundaries.
* [BRIDGE](BRIDGE.md): finite relation identities, the fresh equality-port theorem,
  independent responsibility premises, counterexamples and future UI callback scope.
* [SOURCES](SOURCES.md): Deng/Hani/Ma, BDD/quantification prior art and recent
  permission/formalization papers, with facts separated from our inference.
* `tools/research/059/tests.rs`: actual raw-TypedIR negatives, exact full relation
  checks and exhaustive small transition images. The oracle imports no candidate
  extractor, operand visitor, BDD or transfer implementation.

## Frozen-source reproduction

Use the exact research implementation commit named in the final result receipt.
The tool's Cargo workspace has relative production path dependencies. **Do not
build it directly against a newer mainline and call that a reproduction.** The
driver explicitly archives the production baseline into a new directory and then
overlays only the chosen research commit's tool, fixtures and documents. Rust
dependencies therefore resolve within that frozen schema-8 source tree. Both
independent interpreters reject other schema versions; unknown new instructions
cannot silently become no-ops.

```sh
# Activate an installed Rust 1.99.1 toolchain; this pure-Rust experiment needs no clang.
python3 tools/research/059/run.py prepare \
  --repo /path/to/zil-rust --candidate FULL_RESEARCH_COMMIT \
  --out /path/to/new-unique-freeze
python3 tools/research/059/run.py verify --out /path/to/new-unique-freeze
python3 tools/research/059/run.py sample --out /path/to/new-unique-freeze \
  --window ACTUAL_ROOT_COORDINATED_WINDOW_ID
```

`prepare` records toolchain/profile and exact commands, runs the bounded research
tests/clippy, builds release, hashes the executable/rlibs/source files, obtains
complete candidate and oracle relations, and freezes eligibility. It rejects
empty function censuses, mismatches or unexplained failures. Unsupported and
resource-limited cases remain in the freeze and are excluded explicitly from
ratios. `sample` verifies those hashes, uses fresh processes with two warmups and
seven measured repetitions in rotating method order, and preserves each original
command/stdout/stderr receipt. OS caches are not flushed. Root coordination is
required because a restricted `/proc` namespace cannot itself establish isolation.

The driver retains process wall time, each function's phase times and structure,
then computes medians/ranges and small-sample paired-bootstrap intervals. No sample
has formal standing until the candidate, recipe and exclusive window are fixed.
The final result and receipt distinguish algorithm evidence, compiler acceptance
context, old schema-7 research 055 and current mainline work. The sampled research
commit is `c0c1dc7746bfb4f5f819b10bc68aff579e8fbf46`; final documentation/evidence
does not change that identity.

## Minimal cold replay without sampling

After restoring the complete Git history, use the following from a configured
Rust 1.99.1 environment. Set two paths; the output must not already exist. This
archives the frozen production source and exact research overlay, builds one fresh
release executable and checks `correlated` using all three methods. It does not run
formal performance samples or compile the schema-9 mainline as this experiment.

```sh
export ZIL_REPLAY_REPO=/path/to/recovered/git
export ZIL_REPLAY_OUT=/path/to/new-059-replay
python3 - <<'PY'
from pathlib import Path
import io, json, os, subprocess, tarfile
repo = Path(os.environ['ZIL_REPLAY_REPO']).resolve()
out = Path(os.environ['ZIL_REPLAY_OUT']).resolve()
out.mkdir(parents=True, exist_ok=False)
tree, target = out / 'source', out / 'target'
tree.mkdir()
base = '6acda08c72dbd3e387893b4260770f0834dcdbe0'
research = 'c0c1dc7746bfb4f5f819b10bc68aff579e8fbf46'
overlay = ['tools/research/059', 'tests/perf/impl059', 'docs/research/059']
for commit, paths in [(base, []), (research, overlay)]:
    archive = subprocess.check_output(['git', 'archive', commit, *paths], cwd=repo)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tf:
        tf.extractall(tree, filter='data')
env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0',
           CARGO_PROFILE_TEST_DEBUG='0', CARGO_PROFILE_RELEASE_DEBUG='0',
           CARGO_BUILD_JOBS='1')
subprocess.run(['cargo', 'build', '--offline', '--locked', '--release',
    '--manifest-path', str(tree / 'tools/research/059/Cargo.toml'),
    '--target-dir', str(target)], cwd=tree, env=env, check=True)
baseline_rows = {}
for method in ['mono', 'cut', 'wire']:
    dest = out / method
    command = [str(target / 'release/cut059'),
        str(tree / 'tests/perf/impl059/correlated.zil'), method, str(dest)]
    run = subprocess.run(command, cwd=tree, env=env, capture_output=True, text=True)
    (out / (method + '-command.json')).write_text(json.dumps(dict(
        command=command, returncode=run.returncode,
        stdout=run.stdout, stderr=run.stderr), indent=2) + '\n')
    assert run.returncode == 0, run.stderr
    data = json.loads(run.stdout)
    assert data['schema'] == 8 and data['method'] == method
    assert [f['id'] for f in data['functions']] == [0, 1, 2]
    for f in data['functions']:
        assert f['outcome'] == 'Complete' and f['fault_relations'] == 0
        rows = (dest / ('f%d.json' % f['id'])).read_bytes()
        assert rows == (dest / ('f%d.oracle.json' % f['id'])).read_bytes()
        if method == 'mono': baseline_rows[f['id']] = rows
        else: assert rows == baseline_rows[f['id']]
print(json.dumps(dict(schema=8, source_baseline=base, research_commit=research,
    methods=3, functions_per_method=3, canonical_files=18, outcome='Complete')))
PY
```

This replay checks the admitted research query. Production's separate result for
the same fixture is `Analyze / SafeReject / E-UNINITIALIZED`, as recorded in the
freeze. Research Complete is not a new compiler acceptance rule.

For an original raw-run restoration, retain `source/`, `freeze/`, `commands/`, all
top-level JSON/JSONL, `target/release/cut059`, and the seven rlibs named in
`identity.json` under `target/release/deps/`. The portable archive and member
manifest in `evidence/impl/059/acceptance.json` select exactly these files.
Run the restored `source/tools/research/059/run.py verify --out RESTORED_RUN`;
the original receipt paths remain historical text and need not exist. The
separate portable member manifest also verifies the retained rlib bytes.

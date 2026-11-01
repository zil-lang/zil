#!/usr/bin/env python3
"""Cache identical SSAUpdaterBulk placement queries without changing its value rewrite."""
import argparse
import difflib
import json
from pathlib import Path
from prepare import HERE, once, sha


def identity(path):
    return {'path': str(path.resolve()), 'sha256': sha(path.read_bytes())}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--jump-threading-source', type=Path, required=True,
                        help='Frozen v1 original/bulk source and manifest directory')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    jt_manifest = args.jump_threading_source / 'source-manifest.json'
    jt = json.loads(jt_manifest.read_text())
    for name, expected in jt['files'].items():
        assert sha((args.jump_threading_source / name).read_bytes()) == expected
    upstream = HERE / 'upstream'
    provenance = json.loads((upstream / 'PROVENANCE.json').read_text())
    original = (upstream / 'SSAUpdaterBulk.cpp').read_text()
    assert sha(original.encode()) == provenance['sha256']
    cpp = once(original, '  DenseMap<BasicBlock *, BBValueInfo> BBInfos;\n', '''  DenseMap<BasicBlock *, BBValueInfo> BBInfos;
  // CFG and dominators do not change during this rewrite batch. PHI placement
  // depends only on the complete defining/using block sets, not values/types.
  // Cache the exact pruned placement and the original reserve bound together.
  struct Placement {
    SmallVector<BasicBlock *, 32> IDFBlocks;
    size_t ReserveBlocks;
  };
  DenseMap<SmallVector<BasicBlock *, 8>, unsigned> PlacementIndices;
  SmallVector<Placement, 4> Placements;
''')
    cpp = once(cpp, '    ForwardIDFCalculator IDF(*DT);\n', '')
    cpp = once(cpp, '    IDF.setDefiningBlocks(DefBlocks);\n', '')
    old = '''    SmallVector<BasicBlock *, 32> IDFBlocks;
    SmallPtrSet<BasicBlock *, 32> LiveInBlocks;
    ComputeLiveInBlocks(UsingBlocks, DefBlocks, LiveInBlocks, PredCache);
    IDF.setLiveInBlocks(LiveInBlocks);
    IDF.calculate(IDFBlocks);

    // Reserve sufficient buckets to prevent map growth. [1]
    BBInfos.reserve(LiveInBlocks.size() + DefBlocks.size());'''
    new = '''    SmallVector<BasicBlock *, 8> Key(DefBlocks.begin(), DefBlocks.end());
    llvm::sort(Key, std::less<BasicBlock *>());
    Key.push_back(nullptr); // A block cannot be null; separates both full sets.
    size_t UsesStart = Key.size();
    Key.append(UsingBlocks.begin(), UsingBlocks.end());
    llvm::sort(Key.begin() + UsesStart, Key.end(), std::less<BasicBlock *>());
    auto [Where, New] =
        PlacementIndices.try_emplace(std::move(Key), Placements.size());
    if (New) {
      Placement P;
      SmallPtrSet<BasicBlock *, 32> LiveInBlocks;
      ComputeLiveInBlocks(UsingBlocks, DefBlocks, LiveInBlocks, PredCache);
      ForwardIDFCalculator IDF(*DT);
      IDF.setDefiningBlocks(DefBlocks);
      IDF.setLiveInBlocks(LiveInBlocks);
      IDF.calculate(P.IDFBlocks);
      P.ReserveBlocks = LiveInBlocks.size() + DefBlocks.size();
      Placements.push_back(std::move(P));
    }
    const Placement &P = Placements[Where->second];
    ArrayRef<BasicBlock *> IDFBlocks = P.IDFBlocks;

    // Reserve sufficient buckets to prevent map growth. [1]
    BBInfos.reserve(P.ReserveBlocks);'''
    cpp = once(cpp, old, new)
    args.out.mkdir(parents=True, exist_ok=False)
    for arm, contents in [('original', original), ('grouped', cpp)]:
        dest = args.out / arm
        dest.mkdir()
        (dest / 'SSAUpdaterBulk.cpp').write_text(contents)
        jt_arm = 'original' if arm == 'original' else 'bulk'
        (dest / 'JumpThreading.cpp').write_bytes((args.jump_threading_source / jt_arm / 'JumpThreading.cpp').read_bytes())
    patch = ''.join(difflib.unified_diff(original.splitlines(True), cpp.splitlines(True),
        fromfile='a/llvm/lib/Transforms/Utils/SSAUpdaterBulk.cpp',
        tofile='b/llvm/lib/Transforms/Utils/SSAUpdaterBulk.cpp'))
    (args.out / 'grouped-bulk.patch').write_text(patch)
    manifest = {'scope': __doc__, 'upstream': provenance,
        'jump_threading_source_manifest': identity(jt_manifest),
        'generator': identity(Path(__file__)), 'public_headers_unchanged': True,
        'files': {str(p.relative_to(args.out)): sha(p.read_bytes())
                  for p in sorted(args.out.rglob('*')) if p.is_file()}}
    manifest_path = args.out / 'source-manifest.json'
    manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
    for arm in ['original', 'grouped']:
        recipe = {'schema': 1, 'upstream_commit': provenance['commit'],
            'public_headers_unchanged': True, 'arm': 'ssa-v4-' + arm,
            'replacements': [{'component': component, 'member': name + '.o',
                'source': identity(args.out / arm / name),
                'source_manifest': identity(manifest_path)} for component, name in [
                    ('lib/libLLVMScalarOpts.a', 'JumpThreading.cpp'),
                    ('lib/libLLVMTransformUtils.a', 'SSAUpdaterBulk.cpp')]]}
        (args.out / (arm + '-recipe.json')).write_text(json.dumps(recipe, indent=2) + '\n')
    print(json.dumps({'source': str(args.out), 'patch_sha256': sha(patch.encode())}))


if __name__ == '__main__':
    main()

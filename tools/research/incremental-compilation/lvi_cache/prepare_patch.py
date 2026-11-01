#!/usr/bin/env python3
"""Prepare the pinned LLVM LVI value-index patch without compiling LLVM."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path


COMMIT = '7a941dfb2877a0b8cf30323fefa85ad019e0b0f5'
SOURCE_SHA = 'acba830547744352e9ec983f3f7574a48e28a736c512fae7721afe0ac746cbe7'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def once(text, old, new):
    assert text.count(old) == 1, (old[:100], text.count(old))
    return text.replace(old, new)


def patch_source(original):
    cpp = once(original, '#include "llvm/ADT/DenseSet.h"\n',
               '#include "llvm/ADT/DenseMap.h"\n#include "llvm/ADT/DenseSet.h"\n')
    cpp = once(cpp,
               '  SmallVector<std::unique_ptr<BlockCacheEntry>> BlockCache;\n',
               '''  SmallVector<std::unique_ptr<BlockCacheEntry>> BlockCache;
  /// Block numbers that have cached each value. Entries may conservatively
  /// survive block/edge invalidation; stale memberships only cause no-op
  /// erasures. Hash-set insertion avoids rescanning a growing block list.
  DenseMap<Value *, SmallDenseSet<unsigned, 2>> ValueBlocks;
''')
    cpp = once(cpp, '  void addValueHandle(Value *Val) {\n',
               '''  void addValueHandle(Value *Val, BasicBlock *BB) {
    ValueBlocks[Val].insert(BB->getNumber());
''')
    assert cpp.count('    addValueHandle(Val);\n') == 2
    cpp = cpp.replace('    addValueHandle(Val);\n',
                      '    addValueHandle(Val, BB);\n')
    cpp = once(cpp, '        addValueHandle(V);\n',
               '        addValueHandle(V, BB);\n')
    cpp = once(cpp, '    BlockCache.clear();\n    ValueHandles.clear();\n',
               '    BlockCache.clear();\n    ValueBlocks.clear();\n    ValueHandles.clear();\n')
    cpp = once(cpp, '''void LazyValueInfoCache::eraseValue(Value *V) {
  for (auto &Elem : BlockCache) {
    if (!Elem)
      continue;

    Elem->LatticeElements.erase(V);
    Elem->OverDefined.erase(V);
    if (Elem->NonNullPointers)
      Elem->NonNullPointers->erase(V);
    if (PerPredRanges)
      Elem->PredecessorLatticeElements->erase(V);
  }
''', '''void LazyValueInfoCache::eraseValue(Value *V) {
  auto BlocksIt = ValueBlocks.find(V);
  if (BlocksIt != ValueBlocks.end()) {
    for (unsigned Number : BlocksIt->second) {
      assert(Number < BlockCache.size());
      auto &Elem = BlockCache[Number];
      if (!Elem)
        continue;

      Elem->LatticeElements.erase(V);
      Elem->OverDefined.erase(V);
      if (Elem->NonNullPointers)
        Elem->NonNullPointers->erase(V);
      if (PerPredRanges)
        Elem->PredecessorLatticeElements->erase(V);
    }
    ValueBlocks.erase(BlocksIt);
  }
''')
    assert cpp.count('addValueHandle(Val, BB);') == 2
    assert cpp.count('addValueHandle(V, BB);') == 1
    assert cpp.count('ValueBlocks[Val].insert(BB->getNumber());') == 1
    # Keep every later invalidation/solver/query implementation byte-identical.
    anchor = 'void LVIValueHandle::deleted() {'
    assert cpp[cpp.index(anchor):] == original[original.index(anchor):]
    assert cpp.count('SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception') == 1
    return cpp


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    assert not out.exists(), 'refuse to overwrite an existing source epoch'
    upstream = Path(__file__).with_name('upstream')
    provenance = json.loads((upstream / 'PROVENANCE.json').read_text())
    assert provenance['commit'] == COMMIT
    for entry in provenance['files']:
        data = (upstream / entry['name']).read_bytes()
        assert digest(data) == entry['sha256'] and len(data) == entry['bytes']
    original = (upstream / 'LazyValueInfo.cpp').read_bytes()
    assert digest(original) == SOURCE_SHA
    patched = patch_source(original.decode()).encode()
    out.mkdir(parents=True)
    for arm, contents in [('original', original), ('indexed', patched)]:
        (out / arm).mkdir()
        (out / arm / 'LazyValueInfo.cpp').write_bytes(contents)
    patch = ''.join(difflib.unified_diff(
        original.decode().splitlines(True), patched.decode().splitlines(True),
        fromfile='a/llvm/lib/Analysis/LazyValueInfo.cpp',
        tofile='b/llvm/lib/Analysis/LazyValueInfo.cpp'))
    (out / 'native.patch').write_text(patch)
    for name in ['PROVENANCE.json', 'LICENSE.TXT']:
        (out / name).write_bytes((upstream / name).read_bytes())
    manifest = {
        'scope': __doc__, 'upstream': provenance,
        'public_headers_unchanged': True,
        'internal_cache_layout_changed': True,
        'single_translation_unit': 'llvm/lib/Analysis/LazyValueInfo.cpp',
        'archive': 'lib/libLLVMAnalysis.a', 'member': 'LazyValueInfo.cpp.o',
        'registration_sites': ['insertResult', 'insertPredecessorResults',
                               'isNonNullAtEndOfBlock initialization'],
        'reverse_membership': 'conservative superset; distinct hashed block numbers',
        'generator_sha256': digest(Path(__file__).read_bytes()),
        'patch_sha256': digest(patch.encode()),
        'files': {str(p.relative_to(out)): digest(p.read_bytes())
                  for p in sorted(out.rglob('*')) if p.is_file()},
    }
    manifest_path = out / 'source-manifest.json'
    manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
    for arm in ['original', 'indexed']:
        source = out / arm / 'LazyValueInfo.cpp'
        recipe = {
            'schema': 1, 'upstream_commit': COMMIT,
            'public_headers_unchanged': True, 'arm': 'lvi-' + arm,
            'replacements': [{
                'component': manifest['archive'], 'member': manifest['member'],
                'source': {'path': str(source), 'sha256': digest(source.read_bytes())},
                'source_manifest': {'path': str(manifest_path),
                                    'sha256': digest(manifest_path.read_bytes())},
            }],
        }
        (out / (arm + '-recipe.json')).write_text(json.dumps(recipe, indent=2) + '\n')
    print(json.dumps({'out': str(out), 'source_manifest': str(manifest_path),
                      'patch_sha256': manifest['patch_sha256']}))


if __name__ == '__main__':
    main()

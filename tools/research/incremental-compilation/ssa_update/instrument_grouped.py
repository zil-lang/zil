#!/usr/bin/env python3
"""Derive a counter-only grouped-Bulk epoch without changing the frozen candidate."""
import argparse
import difflib
import json
import shutil
from pathlib import Path
from prepare import once, sha


def ident(path):
    return {'path': str(path.resolve()), 'sha256': sha(path.read_bytes())}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    parent = a.source / 'source-manifest.json'
    manifest = json.loads(parent.read_text())
    for name, expected in manifest['files'].items():
        assert sha((a.source / name).read_bytes()) == expected
    shutil.copytree(a.source, a.out)
    path = a.out / 'grouped/SSAUpdaterBulk.cpp'
    original = path.read_text()
    text = once(original, '#define DEBUG_TYPE "ssaupdaterbulk"', '''#define DEBUG_TYPE "ssaupdaterbulk"
struct GroupedSSACounters {
  unsigned long long Calls = 0, Vars = 0, Hits = 0, Misses = 0;
  unsigned long long LiveIn = 0, Phis = 0;
  ~GroupedSSACounters() {
    errs() << "[ssa-grouped-bulk] calls=" << Calls << " vars=" << Vars
           << " hits=" << Hits << " misses=" << Misses
           << " livein_walk_blocks=" << LiveIn << " inserted_phis=" << Phis
           << "\\n";
  }
};
static GroupedSSACounters GroupedCounts;''')
    text = once(text, '  DenseMap<BasicBlock *, BBValueInfo> BBInfos;', '  ++GroupedCounts.Calls;\n  GroupedCounts.Vars += Rewrites.size();\n  DenseMap<BasicBlock *, BBValueInfo> BBInfos;')
    text = once(text, '    if (New) {\n      Placement P;', '    if (New) {\n      ++GroupedCounts.Misses;\n      Placement P;')
    text = once(text, '      P.ReserveBlocks = LiveInBlocks.size() + DefBlocks.size();', '      GroupedCounts.LiveIn += LiveInBlocks.size();\n      P.ReserveBlocks = LiveInBlocks.size() + DefBlocks.size();')
    text = once(text, '      Placements.push_back(std::move(P));\n    }', '      Placements.push_back(std::move(P));\n    } else {\n      ++GroupedCounts.Hits;\n    }')
    text = once(text, '    ArrayRef<BasicBlock *> IDFBlocks = P.IDFBlocks;', '    ArrayRef<BasicBlock *> IDFBlocks = P.IDFBlocks;\n    GroupedCounts.Phis += IDFBlocks.size();')
    path.write_text(text)
    patch = ''.join(difflib.unified_diff(original.splitlines(True), text.splitlines(True), fromfile='v4/SSAUpdaterBulk.cpp', tofile='v4-counts/SSAUpdaterBulk.cpp'))
    (a.out / 'counter-only.patch').write_text(patch)
    manifest['scope'] = __doc__
    manifest['parent_source_manifest'] = ident(parent)
    manifest['instrumentation_generator'] = ident(Path(__file__))
    manifest['files'] = {str(p.relative_to(a.out)): sha(p.read_bytes()) for p in sorted(a.out.rglob('*')) if p.is_file() and p.name != 'source-manifest.json'}
    dest = a.out / 'source-manifest.json'
    dest.write_text(json.dumps(manifest, indent=2) + '\n')
    recipe = json.loads((a.source / 'grouped-recipe.json').read_text())
    recipe['arm'] = 'ssa-v4-grouped-counters-only'
    for r in recipe['replacements']:
        r['source'] = ident(a.out / 'grouped' / Path(r['source']['path']).name)
        r['source_manifest'] = ident(dest)
    (a.out / 'counts-recipe.json').write_text(json.dumps(recipe, indent=2) + '\n')


if __name__ == '__main__':
    main()

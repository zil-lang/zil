#!/usr/bin/env python3
"""Derive a counter-only epoch from a frozen demand-IDF candidate; never overwrite it."""
import argparse
import difflib
import json
import shutil
from pathlib import Path
from prepare import once, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    manifest_path = args.source / 'source-manifest.json'
    manifest = json.loads(manifest_path.read_text())
    for name, expected in manifest['files'].items():
        assert sha((args.source / name).read_bytes()) == expected, name
    shutil.copytree(args.source, args.out)
    path = args.out / 'bulk/JumpThreading.cpp'
    original = path.read_text()
    counters = '''struct DemandSSACounters {
  unsigned long long Calls = 0, Empty = 0, Debug = 0, Lifetime = 0;
  unsigned long long Vars = 0, Uses = 0, Frontier = 0, Guarded = 0;
  unsigned long long Accepted = 0, Planned = 0, Inserted = 0, Surviving = 0;
  ~DemandSSACounters() {
    errs() << "[ssa-demand-idf] calls=" << Calls << " empty=" << Empty
           << " debug=" << Debug << " lifetime=" << Lifetime
           << " vars=" << Vars << " uses=" << Uses << " frontier=" << Frontier
           << " guarded=" << Guarded << " accepted=" << Accepted
           << " planned=" << Planned << " inserted=" << Inserted
           << " surviving=" << Surviving << "\\n";
  }
};
static DemandSSACounters DemandCounts;
'''
    text = once(original, '// Research: demand-selected shared IDF,', counters + '// Research: demand-selected shared IDF,')
    text = once(text, '  struct Station {', '  ++DemandCounts.Calls;\n  struct Station {')
    text = once(text, '        }))\n      return false;\n    DebugValues.clear();', '        })) {\n      ++DemandCounts.Debug;\n      return false;\n    }\n    DebugValues.clear();')
    text = once(text, '      if (User->isLifetimeStartOrEnd())\n        return false;', '      if (User->isLifetimeStartOrEnd()) {\n        ++DemandCounts.Lifetime;\n        return false;\n      }')
    text = once(text, '  if (Rewrites.empty())\n    return true;', '  if (Rewrites.empty()) {\n    ++DemandCounts.Empty;\n    return true;\n  }\n  DemandCounts.Vars += Rewrites.size();\n  DemandCounts.Uses += NumUses;')
    text = once(text, '  IDF.calculate(Frontier);', '  IDF.calculate(Frontier);\n  DemandCounts.Frontier += Frontier.size();')
    text = once(text, '      if (PlannedPhis > NumUses)\n        return false;', '      if (PlannedPhis > NumUses) {\n        ++DemandCounts.Guarded;\n        DemandCounts.Planned += PlannedPhis;\n        return false;\n      }')
    text = once(text, '  SmallVector<PHINode *, 16> NewPhis;', '  ++DemandCounts.Accepted;\n  DemandCounts.Planned += PlannedPhis;\n  DemandCounts.Inserted += PlannedPhis;\n  SmallVector<PHINode *, 16> NewPhis;')
    text = once(text, '      ++Remaining[PN->getParent()];', '      ++DemandCounts.Surviving;\n      ++Remaining[PN->getParent()];')
    path.write_text(text)
    patch = ''.join(difflib.unified_diff(original.splitlines(True), text.splitlines(True), fromfile='v3/JumpThreading.cpp', tofile='v3-counts/JumpThreading.cpp'))
    (args.out / 'counter-only.patch').write_text(patch)
    manifest['candidate'] += '; diagnostic counters only (not performance arm)'
    manifest['parent_source_manifest_sha256'] = sha(manifest_path.read_bytes())
    manifest['instrumentation_generator_sha256'] = sha(Path(__file__).read_bytes())
    manifest['files'] = {str(p.relative_to(args.out)): sha(p.read_bytes()) for p in sorted(args.out.rglob('*')) if p.is_file() and p.name != 'source-manifest.json'}
    (args.out / 'source-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'source': str(args.out), 'counter_patch_sha256': sha(patch.encode())}))


if __name__ == '__main__':
    main()

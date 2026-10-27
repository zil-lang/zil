#!/usr/bin/env python3
"""Bind existing size/weight plan to exact source files and request binaries."""
import argparse,pathlib,json,hashlib
p=argparse.ArgumentParser();p.add_argument('--experiment',required=True)
for k in ['baseline','candidate','baseline-source','candidate-source']:p.add_argument('--'+k,required=True)
a=p.parse_args();root=pathlib.Path(a.experiment).resolve();out=root/'frozen150.json'
if out.exists():raise SystemExit('refuse to overwrite frozen plan')
d=json.loads((root/'frozen.json').read_text());d.update(schema='zil150-scale/1',variants={'A':'compose149 unchanged','C':'150 private construction identities'},blocks=6,preflight_correction='fixed 141 size family/weights reused before 150 timing; selection is functional success only')
bind={}
for key,src,binary in [('A',a.baseline_source,a.baseline),('C',a.candidate_source,a.candidate)]:
 src=pathlib.Path(src).resolve();binary=pathlib.Path(binary).resolve()
 files={str(p.relative_to(src)):hashlib.sha256(p.read_bytes()).hexdigest()for p in (src/'crates').rglob('*.rs')}
 bind[key]={'source_files':files,'binary':str(binary),'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'embedded_identity':'externally bound by exact source files and executable digest'}
d['binary_bindings']=bind;out.write_text(json.dumps(d,indent=2)+'\n')

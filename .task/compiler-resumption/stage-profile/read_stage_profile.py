#!/usr/bin/env python3
"""Read-only timeline accounting. Subtract interval unions on the SAME thread.
Parallel object spans remain separately labelled; never subtract their sum.
"""
import argparse, collections, json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('run',type=Path);a=p.parse_args()
def union(intervals):
    intervals=sorted(intervals);total=0;end=None
    for lo,hi in intervals:
        if end is None or lo>end:total+=hi-lo;end=hi
        elif hi>end:total+=hi-end;end=hi
    return total
for request in ['cold','warm','private']:
    root=a.run/request;rows=[]
    for line in (root/'stderr').read_text().splitlines():
        try:r=json.loads(line)
        except ValueError:continue
        if r.get('zil_profile')==1:rows.append(r)
    spans=[r for r in rows if r['kind']=='span'];record=json.loads((root/'record.json').read_text());print('\n'+request, 'outer_wall_ms',round(record['wall_ns']/1e6,3),'spans',len(spans))
    totals=[r for r in spans if r['name']=='driver.path.total'];assert len(totals)==1,'missing or duplicate path timer';outer=totals[0]
    print('main thread spans: inclusive_ms / own_thread_uncovered_ms (nested interval unions, no parallel subtraction)')
    for r in sorted((r for r in spans if r['thread']==outer['thread']),key=lambda r:r['start_ns']):
        children=[(s['start_ns'],s['end_ns']) for s in spans if s is not r and s['thread']==r['thread'] and s['start_ns']>=r['start_ns'] and s['end_ns']<=r['end_ns'] and (s['start_ns'],s['end_ns'])!=(r['start_ns'],r['end_ns'])]
        print(f"{r['name']:44} {r['wall_ns']/1e6:10.3f} / {(r['wall_ns']-union(children))/1e6:10.3f}")
    unit=[r for r in spans if r['name']=='backend.object_unit.total'];print('object unit sum_ms / union_wall_ms',round(sum(r['wall_ns'] for r in unit)/1e6,3),round(union((r['start_ns'],r['end_ns']) for r in unit)/1e6,3))
    nc=record['native_codegen'];print('units/cache',nc['actual_units'],nc['object_cache'])
    for k in ['preflight_ns','builder_ns','optimize_ns','emit_ns']:
        print('object reports',k,'sum_ms',round(sum(r.get(k,0) for r in nc['object_reports'])/1e6,3),'(inclusive per-object diagnostic only)')
    previous=outer['start_ns'];print('observer completion intervals (not exclusive phase timings):')
    for r in rows:
        if r['kind']=='event':print(r['phase'],r['disposition'],round((r['at_ns']-previous)/1e6,3));previous=r['at_ns']
    print('after-last-event until path return ms',round((outer['end_ns']-previous)/1e6,3))

import json
from pathlib import Path
import sys
base=Path(__file__).parent
rows={r['name']:r for r in json.loads(Path(sys.argv[1]).read_text())}
expected=json.loads((base/'EXPECTATIONS.json').read_text())['cases']
failures=[]
for name,wanted in expected.items():
    actual=rows[name]['result']
    if actual['accepted'] != wanted['accepted']:
        failures.append({'name':name,'expected':wanted,'actual':actual});continue
    if wanted['accepted']:
        if actual['phase'] != 'Types':failures.append({'name':name,'reason':'positive did not reach Types','actual':actual})
    else:
        if actual['phase']=='Parse' or wanted['code'] not in actual['diagnostics'] or wanted.get('message','') not in actual['diagnostics']:
            failures.append({'name':name,'expected':wanted,'actual':actual})
print(json.dumps({'selected':len(expected),'passed':len(expected)-len(failures),'failed':len(failures),'failures':failures},indent=2))
sys.exit(bool(failures))

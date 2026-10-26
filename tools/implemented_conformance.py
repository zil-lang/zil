"""Keep already implemented language cases mandatory inside unfinished owners."""
import json

FIXED = {
    'T02': ('RX05', 'RX14', 'RX18', 'RX19', 'CL31'),
    'T08': ('PP01', 'PP11', 'CL01', 'CL03', 'CL13', 'CL14', 'CL27', 'CL29', 'CL30'),
}


def check(root, summaries):
    errors = []
    observed = {}
    registrations = {}
    try:
        for case in json.loads((root / 'conformance/index.json').read_text())['cases']:
            if case['id'] in registrations:
                raise ValueError('duplicate registration ' + case['id'])
            registrations[case['id']] = case
    except (KeyError, TypeError, ValueError, OSError) as error:
        errors.append('registration: ' + str(error))
    for owner in FIXED:
        try:
            path = (root / json.loads(summaries[owner])['result_path']).resolve()
            if not path.is_relative_to(root):
                raise ValueError('result escaped project')
            report = json.loads(path.read_text())
            if report.get('task') != owner:
                raise ValueError('wrong task report')
            for case in report['cases']:
                if case['id'] in observed:
                    raise ValueError('duplicate observed case ' + case['id'])
                observed[case['id']] = (owner, case)
        except (KeyError, TypeError, ValueError, OSError) as error:
            errors.append(owner + ': ' + str(error))
    details = []
    for owner, identifiers in FIXED.items():
        for identifier in identifiers:
            registration = registrations.get(identifier, {})
            actual_owner, actual = observed.get(identifier, (None, {}))
            executed, assertions = actual.get('executed', 0), actual.get('assertions', 0)
            passed = (registration.get('owner') == owner and actual_owner == owner
                      and actual.get('status') == 'Pass'
                      and type(executed) is int and executed >= 1
                      and type(assertions) is int and assertions >= 3)
            details.append({'id': identifier, 'owner': owner, 'passed': passed,
                            'executed': executed, 'assertions': assertions})
            if not passed:
                errors.append(identifier + ': missing or nonpassing implemented case')
    return {'name': 'implemented-conformance', 'required_for_scalar_draft': True,
            'passed': not errors, 'exit_code': int(bool(errors)),
            'kind': 'Assertions over this run\'s complete conformance reports; no second execution',
            'cases': details, 'errors': errors}

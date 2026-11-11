#!/usr/bin/env python3
"""Retire only enumerated, archived, inactive compiler refs with atomic leases."""
import datetime
import json
import os
from pathlib import Path
import subprocess
import urllib.error
import urllib.parse
import urllib.request

REPOSITORY = 'zil-lang/zil'
ARCHIVE = 'e75ade949d559bd90914df0d40b860af5f283626'
CANDIDATES = {
    'task/resource-helper-codegen': 'f38ed14b2c18c587f01a117f3c8a6d9cc67c4e7b',
    'task/normalized-function-equivalence': '130d3a7cab25603ae42a7fae0c9f63116c858c72',
}


def api(path):
    request = urllib.request.Request('https://api.github.com/repos/' + REPOSITORY + '/' + path,
                                     headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'],
                                              'Accept': 'application/vnd.github+json',
                                              'X-GitHub-Api-Version': '2022-11-28'})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def main():
    assert os.environ['GITHUB_REPOSITORY'] == REPOSITORY
    output = Path(os.environ['RUNNER_TEMP']) / 'compiler-retirement'
    output.mkdir(parents=True, exist_ok=True)
    receipt = {'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
               'recipe': os.environ['GITHUB_SHA'], 'archive': ARCHIVE, 'results': []}
    try:
        archive = api('git/ref/heads/archive/source-history')['object']['sha']
        receipt['observed_archive'] = archive
        assert archive == ARCHIVE, 'archive moved; re-inspect before retiring any ref'
        parents = {row['sha'] for row in api('git/commits/' + ARCHIVE)['parents']}
        for branch, expected in CANDIDATES.items():
            assert branch.startswith('task/') and expected in parents
            ref = 'refs/heads/' + branch
            head = api('git/ref/heads/' + urllib.parse.quote(branch, safe='/'))['object']['sha']
            assert head == expected, 'branch moved; refusing retirement: ' + branch
            encoded = urllib.parse.quote(branch, safe='')
            checked = {'ref': ref, 'expected': expected, 'observed': head, 'direct_archive_parent': True,
                       'active_runs': {}}
            receipt['results'].append(checked)
            for status in ['queued', 'in_progress', 'waiting', 'requested', 'pending']:
                runs = api('actions/runs?branch=' + encoded + '&status=' + status + '&per_page=100')
                checked['active_runs'][status] = runs['total_count']
                assert runs['total_count'] == 0, 'active workflow still owns ' + branch
            pulls = api('pulls?state=open&head=zil-lang:' + encoded + '&per_page=100')
            checked['open_head_prs'] = len(pulls)
            assert not pulls, 'open pull request still owns ' + branch
            base_pulls = api('pulls?state=open&base=' + encoded + '&per_page=100')
            checked['open_base_prs'] = len(base_pulls)
            assert not base_pulls, 'open pull request targets ' + branch
            result = subprocess.run(['git', 'push', '--porcelain', '--force-with-lease=' + ref + ':' + expected,
                                     'origin', ':' + ref], text=True, capture_output=True)
            row = checked
            row.update(returncode=result.returncode, stdout=result.stdout, stderr=result.stderr)
            assert result.returncode == 0, 'leased deletion failed'
            try:
                api('git/ref/heads/' + urllib.parse.quote(branch, safe='/'))
            except urllib.error.HTTPError as error:
                assert error.code == 404
                row['verified_absent'] = True
            else:
                raise AssertionError('deleted ref still exists')
        receipt['archive_after'] = api('git/ref/heads/archive/source-history')['object']['sha']
        assert receipt['archive_after'] == ARCHIVE, 'archive changed during retirement; re-inspect receipt'
        receipt['status'] = 'Completed enumerated retirements'
    except Exception as error:
        receipt.update(status='Stopped', error=repr(error))
        raise
    finally:
        (output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()

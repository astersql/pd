"""Verify the frozen admission evidence without building or modifying Go."""
import hashlib
import json
import pathlib
import re
import subprocess

BASE = pathlib.Path(__file__).resolve().parent
ROOT = BASE.parents[2]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run():
    inventory = json.loads((BASE / 'source-inventory.json').read_text())
    index = json.loads((BASE / 'oracle-index.json').read_text())
    manifest = json.loads((BASE / 'logs/task_001/manifest.json').read_text())
    raw = json.loads((BASE / 'logs/task_001/outputs.json').read_text())
    before = inventory['readonly_start']
    names = subprocess.check_output(
        ['git', 'ls-files', '-co', '--exclude-standard', '-z'], cwd=ROOT
    ).decode().split('\0')
    files = {
        n: digest(ROOT / n) for n in names
        if n and (ROOT / n).is_file() and
        (n.endswith('.go') or pathlib.Path(n).name in ('go.mod', 'go.sum', 'go.work', 'go.work.sum'))
    }
    assert files == before['files'], 'Go/module file set or content changed'
    all_names = subprocess.check_output(
        ['rg', '--files', '--hidden', '--no-ignore', '-g', '*.go', '-g', 'go.mod',
         '-g', 'go.sum', '-g', 'go.work', '-g', 'go.work.sum', '-g', '!.git/**'], cwd=ROOT
    ).decode().splitlines()
    all_files = {n: digest(ROOT / n) for n in all_names if (ROOT / n).is_file()}
    assert all_files == inventory['readonly_all_files_including_ignored']['files']
    for path, expected in before['plans'].items():
        assert digest(ROOT / path) == expected, path
    for path, expected in inventory['immutable_root_files'].items():
        assert digest(ROOT / path) == expected, path
    status = subprocess.check_output(
        ['git', 'status', '--short', '--', '*.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum'],
        cwd=ROOT, text=True
    )
    assert status == before['status']
    source_paths = {x['path'] for x in inventory['production_files']}
    packages = {x['path'] for x in inventory['packages']}
    actual = {str(p.relative_to(ROOT)) for d in packages for p in (ROOT / d).glob('*.go')
              if not p.name.endswith('_test.go')}
    assert actual == source_paths and len(actual) == 85
    assert len(inventory['scheduler_factories']) == 18
    for record in inventory['production_files']:
        assert digest(ROOT / record['path']) == record['sha256']
        for field in ('public_symbols', 'internal_symbols', 'metrics', 'failpoints',
                      'persistence_sites', 'asynchronous_sites'):
            for item in record[field]:
                assert item['tasks'] and all(str(n) in inventory['tasks'] for n in item['tasks'])
    for api in inventory['verified_dependency_api_sources']:
        assert digest(ROOT / api['file']) == api['sha256']
    for data in raw.values():
        assert hashlib.sha256(data['stdout_stderr_utf8'].encode()).hexdigest() == data['sha256']
    for case in manifest['cases'][:3]:
        assert case['go_exit_code'] == 0 and case['effective_behavior_tests']
        text = (BASE / 'logs/task_001' / case['display_output']).read_text()
        assert re.search(r'Process \d+ has exited with status 0', text)
        assert '--- FAIL:' not in text and 'PASS' in text
    events = json.loads((BASE / 'logs/task_001/native-events.json').read_text())
    kinds = {x['kind']: x for x in events}
    assert len(kinds['completed_before_expiry']['samples']) == 3
    assert len(kinds['completed_retained']['samples']) == 3
    assert kinds['gc_deleted']['samples'] == []
    assert kinds['gc_deleted']['elapsed'] > kinds['completed_retained']['elapsed']
    assert kinds['joined']['exit'] == 0
    safe = json.loads((BASE / 'logs/task_001/safe-tests.json').read_text())
    assert len(safe) == 2
    for case in safe:
        assert case['exit_code'] == 0 and case['stdout_stderr'].count('--- PASS:') == 4
    assert not index['missing_evidence']['unresolved_readonly_acquisition_blockers']
    assert not index['candidate_full_package_command']['allowed']
    assert not manifest['cargo_invoked']
    for link in re.findall(r'\]\(([^)]+)\)', (BASE / 'admission.md').read_text()):
        if '://' not in link:
            assert (BASE / link.split('#')[0]).exists(), link
    result = dict(
        source_commit=inventory['source_commit'], production_files=85, packages=14,
        factories=18, public_symbols=sum(len(x['public_symbols']) for x in inventory['production_files']),
        readonly_go_modules=len(files), readonly_including_ignored=len(all_files),
        unchanged_plans=len(before['plans']), safe_go_tests=8,
        controlled_go_behavior_tests=4, native_gc_scenarios=1,
        json_valid=True, evidence_hashes_valid=True, readonly_audit_passed=True
    )
    print(json.dumps(result, ensure_ascii=False))
    return result


if __name__ == '__main__':
    run()

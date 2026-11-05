"""Read-only source audit; writes only acceptance JSON. No build or domain admission."""
import collections
import gzip
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[4]
OUT = Path(__file__).resolve().parent

def sha(data):
    return hashlib.sha256(data).hexdigest()

def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)

def guard():
    paths = set(git('ls-files', '-z', '--cached', '--others', '--exclude-standard').decode().split('\0'))
    # Include ignored Go/module files as required by the read-only boundary.
    paths.update(git('ls-files', '-z', '--others', '--ignored', '--exclude-standard').decode().split('\0'))
    return {p: sha((ROOT / p).read_bytes()) if (ROOT / p).is_file() else None
            for p in sorted(paths) if p.endswith('.go') or Path(p).name in
            {'go.mod', 'go.sum', 'go.work', 'go.work.sum'}}

before = guard()
rows = []
manifests = {}
inputs = {}
missing = []
for kind, count, task in [('source', 1909, 19), ('wire', 365, 20), ('surface', 267, 21)]:
    name = f'docs/rust/server-api/{kind}-inventory.md'
    data = (ROOT / name).read_bytes()
    inputs[name] = sha(data)
    lines = data.decode().splitlines()
    header = None
    section = ''
    manifest = []
    for i, line in enumerate(lines):
        if line.startswith('#'):
            section = line
        raw_line = line
        plain = re.fullmatch(r'([0-9a-f]{64})  (.+)', line) if kind == 'wire' else None
        if plain:
            digest, path = plain.groups()
            header = ['来源', 'SHA-256', 'owner']
            line = f'| [{path}](../../../{path}) | {digest} | manifest审计（原文无owner列） |'
        if not line.startswith('|'):
            header = None
            continue
        cells = [s.strip() for s in re.split(r'(?<!\\)\|', line.strip().strip('|'))]
        if i + 1 < len(lines) and re.match(r'^\|[ :\-|]+\|$', lines[i + 1]):
            header = cells
            continue
        if not header or all(re.fullmatch(r'[: -]+', c) for c in cells):
            continue
        assert len(header) == len(cells), (name, i + 1)
        owners = [cells[j] for j, h in enumerate(header) if 'owner' in h.lower() or '责任' in h]
        owner102 = any(re.search(r'(?<!\d)102(?!\d)', o) for o in owners)
        hashes = re.findall(r'\b[0-9a-f]{64}\b', line)
        links = re.findall(r'\]\(([^)]+)\)', line)
        is_manifest = bool(hashes) and bool(links) and ('manifest' in section.lower() or '来源集合 SHA' in section)
        fps = []
        for link in links:
            if '://' in link or link.startswith('#'):
                continue
            target = ((ROOT / name).parent / link.split('#')[0]).resolve()
            exists = target.is_file()
            # Historical table candidates may be absent: retain rather than certify.
            fp = {'reference': link, 'exists': exists}
            if exists:
                fp.update(path=str(target.relative_to(ROOT)), current_sha256=sha(target.read_bytes()))
            else:
                missing.append({'inventory': name, 'line': i + 1, 'reference': link})
            fps.append(fp)
        if is_manifest:
            assert len(hashes) == 1 and len(fps) == 1, (name, i+1, line)
            manifest.append({'path': fps[0].get('path', links[0]), 'historical_sha256': hashes[0],
                             **fps[0], 'historical_owner': owners, 'acceptance_task': task})
        # All manifest paths, explicit owner102 rows, all protocol/HTTP rows remain traceable.
        if not (owner102 or is_manifest or (kind == 'wire' and section in
                {'## PD descriptor 全方法', '## 额外服务与 descriptor 注册差异', '## HTTP 全量注册行'})):
            continue
        acceptance = task
        implementation = '原拥有者：' + '；'.join(owners) + '；原102仅审计，不复制实现'
        reason = '未执行本轮业务差分；历史表格状态不是当前准入证明'
        if kind == 'wire' and not is_manifest:
            text = ' '.join(cells[:3])
            if '额外服务' in section:
                acceptance = 14 if '/search_log' in text else 15 if '/server_info' in text else 16
                implementation = '控制面131/138/148及各服务原任务；诊断由本计划14/15负责'
            elif 'PD descriptor' in section:
                acceptance = 22
                method = cells[0]
                if 'gc_service.go' in text and not method.startswith('Watch'):
                    acceptance = (4 if 'V2' in method else 5 if 'Advance' in method else
                                  7 if 'Global' in method and 'Barrier' in method else
                                  6 if 'Barrier' in method else 8 if 'State' in method else 3)
                    implementation = '控制面43–46/53/132；禁止传输层复制GC内核'
            else:
                acceptance = 23
                path = cells[1]
                if '/gc/' in path: acceptance = 8
                elif 'meta-service-groups' in path: acceptance = 12
                elif '/ms/' in path: acceptance = 13
                elif 'keyspace-groups' in path:
                    acceptance = 11 if any(s in path for s in ['split', 'merge', 'keyspaces']) else 10
                elif 'keyspaces' in path: acceptance = 9
                implementation = {8:'控制面43–53/132',9:'控制面36–41/131',10:'控制面29–35/76/131',11:'控制面31–34/42/76/82/131/144',12:'控制面27/28/131',13:'控制面58–64/82/136/140'}.get(acceptance, implementation)
        rid = kind + ':' + sha((section + '\n' + '\n'.join(cells)).encode())[:24]
        rows.append({'id': rid, 'inventory': name, 'line': i + 1, 'section': section,
                     'columns': header, 'cells': cells, 'owner102': owner102,
                     'raw_row': raw_line, 'row_sha256': sha(raw_line.encode()), 'fingerprints': fps,
                     'implementation_owner': implementation, 'acceptance_task': acceptance,
                     'evidence': 'docs/rust/server-api/task-102-resume-evidence.json.gz',
                     'evidence_scope': '历史记录，见task-01.md；不是本轮业务执行',
                     'admission': '未准入', 'reason': reason})
    assert len(manifest) == count, (kind, len(manifest))
    assert len({m['path'] for m in manifest}) == count
    # Deleted upstream plan files are current facts, not admission evidence.
    manifests[kind] = manifest
assert len({r['id'] for r in rows}) == len(rows), 'duplicate stable ID'
history_path = '.plans/2026-10-27-pd-rust-服务端与API/102-完成工作区与来源覆盖审计.md'
historical = git('show', '77fe6a793aeebd15637b8fe02d5dfd74fe0c21ac:' + history_path)
assert sha(historical) == '5396c8c3dbcd9699809595676cbb1846feb370d44a76462ce7655ec8226a47e4'
evidence = 'docs/rust/server-api/task-102-resume-evidence.json.gz'
raw = (ROOT / evidence).read_bytes()
old = json.loads(gzip.decompress(raw))
inputs[evidence] = sha(raw)
for p in ['docs/rust/server-api/task-102.md', 'docs/rust/control-plane-mcs/coverage.md']:
    inputs[p] = sha((ROOT / p).read_bytes())
historical_review = {}
for key in ['tail102_source_audit_2026_11_03', 'tail102_final_regression_2026_11_04']:
    section = old[key]
    historical_review[key] = {
        'source_evidence': evidence, 'json_pointer': '/' + key,
        'section_sha256': sha(json.dumps(section, ensure_ascii=False, sort_keys=True).encode()),
        'command': section.get('command'), 'exit_code': section.get('exit_code'),
        'results': section.get('results'), 'workspace_counts': section.get('workspace_counts'),
        'combined_counts': section.get('combined_counts'), 'source_head': section.get('source_head'),
        'source_hashes': section.get('source_hashes'),
        'source_hash_review': {p: {'historical': digest,
                                  'current': sha((ROOT / p).read_bytes()),
                                  'matches': digest == sha((ROOT / p).read_bytes())}
                               for p, digest in section.get('source_hashes', {}).items()},
        'fingerprints': section.get('fingerprints'),
        'raw_logs': {name: sha(json.dumps(value, ensure_ascii=False).encode())
                     for name, value in section.get('logs', {}).items()},
        'raw_reports_pointer': '/' + key + '/reports',
        'limits': '历史命令/响应/持久状态在原压缩证据原位保留；本轮不执行业务测试',
    }
with gzip.GzipFile(filename=str(OUT / 'task-01-history.json.gz'), mode='wb', mtime=0) as stream:
    stream.write(json.dumps(historical_review, ensure_ascii=False).encode())
for m in manifests.values():
    for item in m:
        item['implementation_owner'] = '原拥有者：' + '；'.join(item['historical_owner']) + '；精确实现责任见原始列及handoff.md'
        item['admission'] = '未准入'
        item['reason'] = '哈希与路径核验不证明行为等价'
after = guard()
assert before == after
result = {'head': git('rev-parse', 'HEAD').decode().strip(), 'historical_task_sha256': sha(historical),
          'inputs': inputs, 'rows': rows, 'manifests': manifests,
          'summary': {'rows': len(rows), 'owner102_rows': sum(r['owner102'] for r in rows),
                      'by_acceptance': dict(collections.Counter(r['acceptance_task'] for r in rows)),
                      'manifest_counts': {k: len(v) for k,v in manifests.items()}},
          'missing_historical_candidate_references': missing,
          'historical_evidence_sections': list(old),
          'go_guard': {'count': len(before), 'before': before, 'after': after, 'equal': before == after},
          'current_behavior_tests': {'passed': 0, 'failed': 0, 'ignored': 0},
          'audit_command': 'python3 docs/rust/server-api/acceptance/audit_handoff.py', 'exit_code': 0}
(OUT / 'coverage.json').write_text(json.dumps(result, ensure_ascii=False, separators=(',', ':')) + '\n')
print(json.dumps(result['summary'], ensure_ascii=False))
print('Go guard:', len(before), 'equal; historical candidate missing refs:', len(missing))

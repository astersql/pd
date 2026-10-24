"""Parse actual Delve returns; never calculate the Go function results here."""
import json
import pathlib
import re

root = pathlib.Path(__file__).resolve().parents[4]
text = (pathlib.Path(__file__).parent / 'oracle.out').read_text()
assert not re.search(r'no member|nil pointer|could not|command not', text)
values = {}
for key, block in re.findall(r'"CASE ([^"]+)"\n(.*?)(?="CASE |\Z)', text, re.S):
    value = re.search(r'~r0: (.*)', block)
    if value:
        raw = value[1]
    elif key in ['read_stats', 'write_stats']:
        raw = block.strip()
    elif key in ['thresholds', 'full_after_write', 'full_after_read']:
        raw = re.search(r'\[8\]float64 (.*)', block)[1]
    else:
        assert key.startswith('set_'), (key, block)
        continue
    if 'RegionStatKind len:' in raw:
        value = [int(x) for x in re.findall(r'\((-?\d+)\)', raw)]
    elif 'float64 len:' in raw:
        value = [x if x in ['NaN', '+Inf', '-Inf'] else float(x) for x in raw.rsplit('[', 1)[1].rstrip(']').split(',')]
    elif raw.startswith('['):
        value = json.loads(raw)
    elif re.fullmatch(r'\w+ \((-?\d+)\)', raw):
        value = int(re.search(r'\((-?\d+)\)', raw)[1])
    elif key == 'single_label':
        value = dict(re.findall(r'"([^"]+)": "([^"]*)"', block))
    else:
        value = json.loads(raw)
    values[key] = value
out = root / 'tests/rust-compat/fixtures/state/task-01.json'
out.write_text(json.dumps({'binary_sha256': '4537466d65d253b1e291cacecf53faee14117386e8f2c5ff7b380108e49cc268', 'values': values}, indent=2, ensure_ascii=False) + '\n')
print(len(values), 'observed Go results parsed')

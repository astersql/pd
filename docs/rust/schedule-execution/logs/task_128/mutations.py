"""Prove the frozen behavioral assertions reject missing effects; restore source on exit."""
from pathlib import Path
import subprocess
import sys
p=Path('pkg/cluster/overlaps.rs')
original=p.read_text()
mutations={
    'stats-red':('stats.clear_defunct_region(id);','let _ = stats;'),
    'labels-red':('labels.mark_defunct_region(id);','let _ = labels;'),
    'affinity-red':('manager.invalid_cache(id);','let _ = manager;'),
    'placement-red':('rules.invalid_cache(id);','let _ = rules;'),
    'cancel-red':('if cancelled() {','if false && cancelled() {'),
}
try:
    for name,(before,after) in mutations.items():
        assert original.count(before)==1
        p.write_text(original.replace(before,after))
        subprocess.run([sys.executable,'docs/rust/schedule-execution/logs/task_128/verify.py',name,'cargo fmt --all','cargo test -p astersql-pd-cluster-utils --locked --lib task_128 -- --nocapture'],check=True)
        import json
        result=json.loads(Path(f'docs/rust/schedule-execution/logs/task_128/{name}.json').read_text())
        assert result[-1]['exit_code']==101 and any('0 passed; 1 failed' in s for s in result[-1]['test_summaries']),result
finally:
    p.write_text(original)

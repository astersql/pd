"""Prove the same registered tests reject incorrect domain behavior; always restore."""
import json
import os
from pathlib import Path
import re
import subprocess

assert os.environ['CARGO_TARGET_DIR'].startswith(str(Path.cwd()/'target/rust-slot-'))
out = Path('docs/rust/schedule-execution/logs/task_004')
path = Path('pkg/schedule/core/cluster_views.rs')
original = path.read_text()
command = ['cargo','test','-p','astersql-pd-schedule-core','--locked','--lib','task_004','--','--nocapture']
mutations = [
    ('store', 'self.store_statistics.get_stores_loads()', 'HashMap::new()', 'task_004_behavior_cases'),
    ('bucket', '.collect(degree, region_ids)', '.collect(degree.max(0), region_ids)', 'task_004_behavior_cases'),
    ('tiflash', '.filter(|store| !store.is_tiflash_write())', '.filter(|_store| true)', 'task_004_checker_label_updates'),
    ('defunct', 'self.label_statistics.clear_defunct_regions();', '// Mutant: omit deferred cleanup.', 'task_004_checker_label_updates'),
]
results = []
try:
    for name, needle, replacement, test in mutations:
        assert original.count(needle) == 1, needle
        path.write_text(original.replace(needle, replacement))
        subprocess.run(['cargo','fmt','--all'],check=True)
        result = subprocess.run(command,capture_output=True,text=True)
        text = result.stdout+result.stderr
        (out/f'red-{name}.out').write_text(text)
        counts = re.search(r'test result: FAILED\. (\d+) passed; (\d+) failed; (\d+) ignored',text)
        results.append(dict(mutation=name,command=command,exit_code=result.returncode,
            passed=int(counts[1]) if counts else 0,failed=int(counts[2]) if counts else 0,
            assertion_test=test,cargo_target_dir=os.environ['CARGO_TARGET_DIR']))
        print(results[-1],flush=True)
        assert result.returncode==101 and counts and int(counts[2]) > 0
        assert test+' ... FAILED' in text and 'assertion `left == right` failed' in text
finally:
    path.write_text(original)
    subprocess.run(['cargo','fmt','--all'],check=True)
    (out/'mutation.json').write_text(json.dumps(results,indent=2)+'\n')

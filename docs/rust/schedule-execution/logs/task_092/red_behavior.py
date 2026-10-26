# A controlled production mutation must fail the same real behavior tests.
from pathlib import Path
import subprocess,os
path=Path('pkg/schedule/schedulers/hot_solve.rs')
original=path.read_bytes()
assert os.environ['CARGO_TARGET_DIR'].startswith(str(Path.cwd()/'target/rust-slot-'))
try:
    text=original.decode()
    old='if !self.search(rank.rank,'
    assert text.count(old)==1
    path.write_text(text.replace(old,'if true || !self.search(rank.rank,'))
    result=subprocess.run(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_092','--','--nocapture'],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    print(result.stdout)
    print(f'controlled Cargo exit code: {result.returncode}')
    assert result.returncode==101
    assert 'task_092_reverse_and_atomic_failure ... FAILED' in result.stdout
    assert 'task_092_reverse_gates_and_search_rounds ... FAILED' in result.stdout
finally:
    path.write_bytes(original)

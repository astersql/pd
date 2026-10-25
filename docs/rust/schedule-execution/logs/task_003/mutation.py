"""Check that the same oracle tests detect lost halt state, then restore source."""
import json
import subprocess
from pathlib import Path
root = Path('docs/rust/schedule-execution/logs/task_003')
source = Path('pkg/schedule/core/shared_cluster.rs')
original = source.read_text()
needle = 'self.shared_config.snapshot().schedule.halt_scheduling || self.recovery_running'
assert needle in original
command = ['cargo', 'test', '-p', 'astersql-pd-schedule-core', '--locked', '--lib', 'task_003', '--', '--nocapture']
try:
    source.write_text(original.replace(needle, 'false'))
    subprocess.run(['cargo', 'fmt', '--all'], check=True)
    result = subprocess.run(command, capture_output=True, text=True)
    (root / 'red-behavior.out').write_text(result.stdout + result.stderr)
    (root / 'mutation.json').write_text(json.dumps(dict(
        mutation='always return false for halt', argv=command, exit_code=result.returncode,
        expected_failure='task_003_halt_and_prepare_error; true != false',
        stdout=result.stdout, stderr=result.stderr,
    ), indent=2) + '\n')
    assert result.returncode == 101
    assert 'task_003_halt_and_prepare_error ... FAILED' in result.stdout
finally:
    source.write_text(original)
    subprocess.run(['cargo', 'fmt', '--all'], check=True)

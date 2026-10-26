# Prove real behavior assertions detect incorrect defaults and queue transitions.
from pathlib import Path
import subprocess, signal

def interrupted(signum, frame):
    raise KeyboardInterrupt
signal.signal(signal.SIGTERM, interrupted)
p = Path('pkg/schedule/schedulers/balance_range_job.rs')
original = p.read_bytes()
try:
    for old, new, test in [
        (b'RANGE_JOB_TIMEOUT: i64 = 1_800_000_000_000', b'RANGE_JOB_TIMEOUT: i64 = 1_800_000_000_001', 'task_104_behavior_cases'),
        (b'job.status = JobStatus::Running;', b'job.status = JobStatus::Pending;', 'task_104_queue_lifecycle'),
    ]:
        assert old in original
        p.write_bytes(original.replace(old, new))
        child = subprocess.Popen(['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib',test,'--','--nocapture'], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        try:
            output, _ = child.communicate()
        except BaseException:
            child.terminate()
            child.wait()
            raise
        print(output)
        print('mutation cargo exit:', child.returncode)
        assert child.returncode == 101 and 'assertion `left == right` failed' in output
finally:
    p.write_bytes(original)

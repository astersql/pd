"""Exercise registered assertions against wrong behavior; restore source always."""
from pathlib import Path
import subprocess,json,sys,signal

def interrupted(signum, frame):
    raise KeyboardInterrupt(f"signal {signum}")
signal.signal(signal.SIGTERM, interrupted)

def run(command, cwd, quiet=False):
    process=subprocess.Popen(command,cwd=cwd,stdout=subprocess.DEVNULL if quiet else None)
    try:
        return process.wait()
    except BaseException:
        process.terminate()
        process.wait() # runner terminates and joins its Cargo process group
        raise

out=Path(__file__).resolve().parent
root=out.parents[4]
p=root/'pkg/schedule/checker/priority_inspector.rs'
original=p.read_text()
mutants=[
 ('red-behavior','self.update(-missing, region.meta().id, now_unix_nanos);','let _ = missing;'),
 ('mutation-due','saturating_add(delay) < now','saturating_add(delay) <= now'),
 ('mutation-learner','.filter(|rf| rf.rule.role != PeerRoleType::Learner)','.filter(|_| true)'),
 ('mutation-recovery','queue.remove(id);','// removed recovery removal'),
 ('mutation-backoff','retry.attempt = retry.attempt.saturating_add(1);','retry.attempt = 1;'),
]
results=[]
try:
    for name,before,after in mutants:
        assert before in original, name
        p.write_text(original.replace(before,after,1))
        assert run([sys.executable,str(out/'run.py'),name+'-fmt','cargo','fmt','--all'],root,quiet=True)==0
        command=[sys.executable,str(out/'run.py'),name,'cargo','test','-p','astersql-pd-schedule-checker','--locked','--lib','task_051','--','--nocapture']
        code=run(command,root,quiet=True)
        log=(out/f'{name}.log').read_text()
        detected=code==101 and 'panicked at' in log and 'test result: FAILED' in log
        results.append(dict(name=name,exit_code=code,behavior_assertion_detected=detected))
        print(json.dumps(results[-1]),flush=True)
        assert detected, log
finally:
    p.write_text(original)
    (out/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')

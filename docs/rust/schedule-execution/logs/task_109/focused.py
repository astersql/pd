import subprocess,sys
for cmd in [['cargo','fmt','--all'],['cargo','test','-p','astersql-pd-schedule-schedulers','--locked','--lib','task_109','--','--nocapture']]:
 print('COMMAND',cmd,flush=True)
 rc=subprocess.call(cmd)
 if rc: sys.exit(rc)

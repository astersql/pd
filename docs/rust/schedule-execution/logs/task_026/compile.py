import subprocess
subprocess.run(["cargo","fmt","--all"],check=True)
raise SystemExit(subprocess.run(["cargo","test","-p","astersql-pd-schedule-operator","--locked","--lib","task_026","--","--nocapture"]).returncode)

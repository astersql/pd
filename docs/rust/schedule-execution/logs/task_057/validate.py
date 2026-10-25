from pathlib import Path
import subprocess,sys,os
runner=Path(__file__).with_name("run.py")
commands=[
 ("focused-provenance",["cargo","test","-p","astersql-pd-schedule-checker","--locked","--lib","task_057","--","--nocapture"]),
 ("package",["cargo","test","-p","astersql-pd-schedule-checker","--locked"]),
 ("metadata",["cargo","metadata","--format-version","1","--no-deps"]),
 ("fmt-check",["cargo","fmt","--all","--","--check"]),
 ("workspace-check",["cargo","check","--workspace","--locked"]),
 ("workspace-test",["cargo","test","--workspace","--locked"])]
for name,cmd in commands:
 env=dict(os.environ)
 if name=="workspace-test": env["RUST_TEST_THREADS"]="2"
 result=subprocess.run([sys.executable,str(runner),name,*cmd],env=env)
 if result.returncode: sys.exit(result.returncode)

#!/usr/bin/env python3
"""Run the Rust real-file integration binary with development runtimes hidden."""
import argparse
import os
from pathlib import Path
import signal
import subprocess
import sys
p=argparse.ArgumentParser();p.add_argument('--app',type=Path,required=True);p.add_argument('--test-binary',type=Path,required=True);p.add_argument('--h5ad',type=Path,required=True);a=p.parse_args()
root=Path(__file__).resolve().parents[1]
python=a.app.resolve()/'Contents/Resources/python-worker/python/bin/python3.12'
assert python.is_file() and a.test_binary.is_file() and a.h5ad.is_file()
listed=subprocess.run([str(a.test_binary.resolve()),'--list'],capture_output=True,text=True,check=True)
if 'h5ad::tests::public_h5ad_resource_stop_retry_restore_and_isolation: test' not in listed.stdout:
 raise SystemExit('Required integration test is absent from the supplied binary')
pairs=[(root/'.venv',root/'.worker-cache/hidden-development-venv'),(root/'src-tauri/python-worker',root/'.worker-cache/hidden-staged-runtime')]
assert all(not hidden.exists() for _,hidden in pairs), 'Unrestored isolation fixture exists'
changed=[]
def stop(*_):raise SystemExit('Isolation interrupted; restoring development directories')
signal.signal(signal.SIGTERM,stop)
try:
 for source,hidden in pairs:
  if source.exists():source.rename(hidden);changed.append((source,hidden))
 assert not (root/'.venv').exists() and not (root/'src-tauri/python-worker').exists()
 env=dict(os.environ,PATH='/usr/bin:/bin',CAUSALMED_PYTHON=str(python),CAUSALMED_TEST_DONOR_H5AD=str(a.h5ad.resolve()),PYTHONHOME='/no-development-python',PYTHONPATH='/no-development-imports',PYTHONDONTWRITEBYTECODE='1')
 subprocess.run([str(a.test_binary.resolve()),'--exact','h5ad::tests::public_h5ad_resource_stop_retry_restore_and_isolation','--ignored','--nocapture'],cwd=root,env=env,check=True)
 print('PASS: real import/QC/export/resource-stop/retry with .venv and staged runtime absent')
finally:
 for source,hidden in reversed(changed):
  if source.exists():raise RuntimeError('Unexpected replacement during isolation: '+str(source))
  hidden.rename(source)
 print('Restored development directories')

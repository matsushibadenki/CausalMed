#!/usr/bin/env python3
"""Audit a relocated .app with a sanitized environment and no development imports."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import bundle_worker as build

p=argparse.ArgumentParser();p.add_argument('app',type=Path);args=p.parse_args()
root=args.app.resolve()/'Contents/Resources/python-worker'
pin=json.loads((build.ROOT/'python/runtime/aarch64-apple-darwin.json').read_text())
lock=build.digest(build.ROOT/'python/runtime/wheels-aarch64-apple-darwin.json')
assert build.verify(root,pin,lock), 'Packaged runtime inventory changed'
env=dict(os.environ,PATH='/usr/bin:/bin',PYTHONHOME='/no-developer-python',PYTHONPATH='/no-developer-imports',VIRTUAL_ENV='/no-venv',CAUSALMED_PYTHON='/no-override')
code="""import sys,json,pathlib,anndata,numpy,h5py,pandas,scipy
root=pathlib.Path(sys.executable).resolve().parents[1]
assert pathlib.Path(sys.prefix).resolve()==root
assert all(pathlib.Path(m.__file__).resolve().is_relative_to(root) for m in [anndata,numpy,h5py,pandas,scipy])
assert all(pathlib.Path(p).resolve().is_relative_to(root) for p in sys.path)
print(json.dumps({'python':sys.version.split()[0],'prefix':str(root),'modules':[m.__name__ for m in [anndata,numpy,h5py,pandas,scipy]],'isolated':sys.flags.isolated}))"""
r=subprocess.run([str(root/'python/bin/python3.12'),'-I','-B','-c',code],env=env,text=True,capture_output=True,check=True)
print(r.stdout.strip())
# Audit Mach-O load paths; system frameworks and bundled loader-relative libraries only.
native=0
for f in root.rglob('*'):
 if not f.is_file() or f.is_symlink():continue
 with f.open('rb') as stream:magic=stream.read(4)
 if magic not in (b'\xcf\xfa\xed\xfe',b'\xce\xfa\xed\xfe',b'\xca\xfe\xba\xbe',b'\xfe\xed\xfa\xcf'):continue
 native+=1
 result=subprocess.run(['/usr/bin/otool','-L',str(f)],text=True,capture_output=True,check=True)
 own_ids=[]
 if f.suffix=='.dylib':
  ids=subprocess.run(['/usr/bin/otool','-D',str(f)],text=True,capture_output=True,check=True)
  own_ids=[line.strip() for line in ids.stdout.splitlines()[1:]]
 dependencies=0
 for line in result.stdout.splitlines():
  if not line.startswith('\t'):continue
  dep=line.strip().split(' (')[0]
  # otool lists a dylib's own install name before its load dependencies.
  own_id=dep in own_ids
  dependencies+=1
  if own_id:continue
  assert dep.startswith(('/usr/lib/','/System/Library/','@loader_path/','@rpath/','@executable_path/')), (f,dep)
print(json.dumps({'verifiedNativeFiles':native,'externalHomebrewDependencies':0,'completeInventory':True}))

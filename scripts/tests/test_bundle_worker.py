import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
spec=importlib.util.spec_from_file_location('bundle_worker',Path(__file__).parents[1]/'bundle_worker.py')
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
class BundleTests(unittest.TestCase):
 def test_offline_artifacts_are_hash_checked(self):
  with tempfile.TemporaryDirectory() as t:
   p=Path(t)/'wheel';p.write_bytes(b'valid');sha=b.digest(p)
   self.assertEqual(b.fetch('unused',sha,p,True),p)
   p.write_bytes(b'changed')
   with self.assertRaises(RuntimeError):b.fetch('unused',sha,p,True)
   self.assertEqual(p.read_bytes(),b'changed')
 def test_inventory_includes_nested_metadata_and_rejects_external_symlinks(self):
  with tempfile.TemporaryDirectory() as t,tempfile.TemporaryDirectory() as other:
   root=Path(t);(root/'package').mkdir();(root/'package/runtime.json').write_text('{}');(root/'runtime.json').write_text('{}')
   self.assertIn('package/runtime.json',b.inventory(root));self.assertNotIn('runtime.json',b.inventory(root))
   (root/'external').symlink_to(Path(other))
   with self.assertRaises(RuntimeError):b.inventory(root)
 def test_complete_inventory_detects_dependency_changes(self):
  with tempfile.TemporaryDirectory() as t:
   root=Path(t);binary=root/'python/bin/python3.12';binary.parent.mkdir(parents=True);binary.write_bytes(b'python')
   pin=dict(pythonVersion='3.12.16',sha256='a'*64)
   (root/'files.json').write_text(json.dumps(b.inventory(root)))
   m=dict(schemaVersion=1,target=b.TARGET,pythonVersion=pin['pythonVersion'],archiveSha256=pin['sha256'],dependencyLockSha256='b'*64,pythonSha256=b.digest(binary),inventorySha256=b.digest(root/'files.json'),buildRecipeSha256=b.digest(Path(b.__file__)))
   (root/'runtime.json').write_text(json.dumps(m));self.assertIsNotNone(b.verify(root,pin,'b'*64))
   (root/'python/dependency.py').write_text('changed');self.assertIsNone(b.verify(root,pin,'b'*64))

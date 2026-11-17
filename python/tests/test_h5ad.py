import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

try:
    import anndata as ad
    import numpy as np
    import pandas as pd
    from scipy import sparse
except ImportError:
    ad = None

from causalmed_worker.h5ad import inspect


@unittest.skipUnless(sys.platform in ('darwin', 'linux'), 'Requires RLIMIT_AS')
class WorkerMemoryTests(unittest.TestCase):
    def test_kernel_denies_mapping_before_it_becomes_resident(self):
        code = """
import mmap
from causalmed_worker import h5ad

def native_allocation(*args):
    # Reserve address space without accessing any pages. An RSS watchdog
    # cannot observe this request as resident memory.
    with mmap.mmap(-1, 1024 * 1024 * 1024):
        raise AssertionError('kernel allowed an over-budget mapping')

h5ad.inspect = native_allocation
raise SystemExit(h5ad.main())
"""
        output = subprocess.run(
            [sys.executable, '-c', code, 'unused.h5ad'],
            env={**os.environ, 'CAUSALMED_WORKER_MEMORY_BYTES': str(128 * 1024 * 1024)},
            capture_output=True, text=True, timeout=15,
        )
        self.assertEqual(output.returncode, 1, output.stderr)
        self.assertEqual(json.loads(output.stdout), {'type': 'error', 'code': 'worker_memory_limit'})


@unittest.skipIf(ad is None, 'Install python/requirements.txt for H5AD tests')
class H5adTests(unittest.TestCase):
    def test_real_hdf5_dense_and_sparse_annotations_and_missing(self):
        for matrix in (np.ones((4, 3)), sparse.csr_matrix(np.ones((4, 3)))):
            with self.subTest(matrix=type(matrix).__name__), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / 'fixture.synthetic.h5ad'
                data = ad.AnnData(matrix, obs=pd.DataFrame({'donor_id': ['D1', 'D1', 'D2', None], 'louvain': pd.Categorical(['A','A','B','B']), 'condition': pd.Categorical(['QA']*4)}, index=['c1','c2','c3','c4']))
                data.write_h5ad(path)
                stages = []
                report = inspect(path, lambda percent, phase: stages.append((percent, phase)))
                self.assertEqual((report['cells'], report['genes']), (4, 3))
                self.assertEqual(report['sha256'], hashlib.sha256(path.read_bytes()).hexdigest())
                self.assertEqual(report['groups']['donor_id']['missing'], 1)
                self.assertEqual(report['groups']['louvain']['counts'][0]['count'], 2)
                self.assertEqual(stages, sorted(stages))
                self.assertEqual(report['obsSchema'][0], {'name':'donor_id','dtype':'category','kind':'categorical','nullable':True})
                self.assertFalse(report['obsSchemaTruncated'])
                self.assertTrue(report['groups']['donor_id']['complete'])
                self.assertEqual(report['qc']['annotationContext']['donor_id']['distinct'], 2)
                self.assertEqual(report['qc']['annotationContext']['donor_id']['counts'], [])
                self.assertEqual(report['qc']['annotationContext']['condition']['counts'], [{'label':'QA','count':4}])
                self.assertTrue(report['qc']['annotationContext']['condition']['complete'])
                self.assertIsNone(report['synthetic'])  # Never infer biological provenance from a filename.

    def test_duplicate_ids_and_category_tail(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'fixture.synthetic.h5ad'
            data = ad.AnnData(np.ones((25, 2)), obs=pd.DataFrame({'condition': [f'C{i}' for i in range(25)]}, index=['duplicate'] * 25))
            data.write_h5ad(path)
            report = inspect(path)
            self.assertEqual(report['duplicateCellIds'], 24)
            group = report['groups']['condition']
            self.assertEqual(len(group['counts']), 20)
            self.assertEqual(group['other'], 5)
            self.assertEqual(sum(r['count'] for r in group['counts']) + group['other'] + group['missing'], 25)

    def test_corrupt_file_returns_protocol_error_and_nonzero_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'broken.h5ad'
            path.write_text('not an HDF5 file')
            output = subprocess.run([sys.executable, '-m', 'causalmed_worker.h5ad', str(path)], capture_output=True, text=True)
            self.assertNotEqual(output.returncode, 0)
            events = [json.loads(line) for line in output.stdout.splitlines()]
            self.assertEqual(events[-1], {'type': 'error', 'code': 'invalid_h5ad'})
            self.assertFalse(any(event['type'] == 'result' for event in events))

    @unittest.skipUnless(os.environ.get('CAUSALMED_TEST_H5AD'), 'Set CAUSALMED_TEST_H5AD to public PBMC3k file')
    def test_public_pbmc3k(self):
        report = inspect(os.environ['CAUSALMED_TEST_H5AD'])
        self.assertEqual((report['cells'], report['genes']), (2638, 1838))
        self.assertIn('louvain', report['groups'])
        self.assertEqual(sum(r['count'] for r in report['groups']['louvain']['counts']), 2638)
        self.assertEqual(report['duplicateCellIds'], 0)

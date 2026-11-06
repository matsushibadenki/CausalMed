import tempfile
import unittest
from pathlib import Path
import anndata as ad
import numpy as np
import pandas as pd
from scipy import sparse
from causalmed_worker.h5ad import inspect
from causalmed_worker.outcome import inspect_outcome

class OutcomeTests(unittest.TestCase):
    def fixture(self, root, encoding='csr'):
        x = np.array([[1, 3], [3, 1], [0, 2], [2, 2]], dtype=np.float32)
        a = ad.AnnData(sparse.csr_matrix(x) if encoding == 'csr' else sparse.csc_matrix(x) if encoding == 'csc' else x,
                       obs=pd.DataFrame({'donor':['a','a','b','b']}, index=['c0','c1','c2','c3']),
                       var=pd.DataFrame(index=['G','MT-X']))
        path = root/'test.h5ad'
        a.write_h5ad(path)
        report = inspect(path, options={'donorColumn':'donor','minGenes':0,'maxMitoPct':None}, artifact_dir=root/'qc')
        obs = dict(qcId=report['selection']['id'], manifestSha256=report['selection']['manifestSha256'],retained=4,missingRetained=0,donors=[dict(label='a',retained=2),dict(label='b',retained=2)])
        ctx = dict(spec=dict(featureId='G',featureColumn='_index'), inputs=[dict(sourceSha256=report['sha256'],cohort=dict(observation=obs))])
        jobs = [dict(path=str(path),selectionDir=str(root/'qc'),selection=report['selection'])]
        return x, ctx, jobs

    def test_dense_csr_csc_equal_donor_means_and_missing_feature(self):
        hashes=[]
        for encoding in ('dense','csr','csc'):
            with self.subTest(encoding=encoding), tempfile.TemporaryDirectory() as tmp:
                x, ctx, jobs = self.fixture(Path(tmp),encoding)
                r = inspect_outcome(ctx,'a'*64,jobs)['inputs'][0]
                self.assertEqual(r['issues'],[])
                expected=np.log1p(x[:,0]/x.sum(axis=1)*10000)
                np.testing.assert_allclose([d['mean'] for d in r['donors']], [expected[:2].mean(),expected[2:].mean()],rtol=1e-7)
                self.assertEqual([d['cells'] for d in r['donors']],[2,2])
                hashes.append(r['featureSpaceSha256'])
                ctx['spec']['featureId']='absent'
                r=inspect_outcome(ctx,'b'*64,jobs)['inputs'][0]
                self.assertEqual(r['issues'],['feature_missing'])
                self.assertEqual(r['donors'],[])
        self.assertEqual(len(set(hashes)),1)

    def test_changed_selection_and_source_abstain(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); _, ctx, jobs=self.fixture(root)
            (root/'qc'/'cells.npz').write_bytes(b'changed')
            self.assertEqual(inspect_outcome(ctx,'a'*64,jobs)['inputs'][0]['issues'],['selection_changed'])
            Path(jobs[0]['path']).write_bytes(b'changed')
            self.assertEqual(inspect_outcome(ctx,'a'*64,jobs)['inputs'][0]['issues'],['file_changed'])

    def test_unavailable_source_is_bounded_abstention(self):
        with tempfile.TemporaryDirectory() as tmp:
            _,ctx,_=self.fixture(Path(tmp))
            r=inspect_outcome(ctx,'a'*64,[dict(issue='file_changed')])['inputs'][0]
            self.assertEqual(r['issues'],['file_changed']);self.assertEqual(r['donors'],[])

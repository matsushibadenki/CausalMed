import json
import os
from pathlib import Path
import tempfile
import unittest

try:
    import anndata as ad
    import numpy as np
    import pandas as pd
    from scipy import sparse
except ImportError:
    ad = None
from causalmed_worker.qc import quality_control, qc_options
from causalmed_worker.h5ad import inspect


@unittest.skipIf(ad is None, 'Install python/requirements.txt for QC tests')
class ExpressionQcTests(unittest.TestCase):
    def make_data(self, constructor=None):
        constructor = constructor or np.asarray
        matrix = np.array([[1,4,0,0],[0,0,0,0],[3,1,1,0],[0,10,20,0],[0,2,0,0]],dtype=float)
        return ad.AnnData(constructor(matrix),obs=pd.DataFrame({'subject':['D1',None,'D2','D2',' ']},index=[f'c{i}' for i in range(5)]),var=pd.DataFrame(index=['MT-ND1','G1','G2','G3']))

    def test_dense_csr_csc_backed_metrics_and_overlapping_filters_agree(self):
        options={'donorColumn':'subject','minGenes':2,'maxGenes':3,'minCounts':3,'maxCounts':20,'maxMitoPct':20}
        for constructor in (np.asarray,sparse.csr_matrix,sparse.csc_matrix):
            with self.subTest(format=constructor.__name__), tempfile.TemporaryDirectory() as directory:
                data=self.make_data(constructor)
                path=Path(directory)/'fixture.synthetic.h5ad'
                data.write_h5ad(path)
                data=ad.read_h5ad(path,backed='r')
                try: q=quality_control(data,options)
                finally: data.file.close()
                self.assertEqual((q['retained'],q['excluded']),(1,4))
                self.assertEqual(q['failures'],{'zeroCounts':1,'lowGenes':2,'highGenes':0,'lowCounts':2,'highCounts':1,'highMito':1})
                self.assertEqual(q['metrics']['counts']['median'],5)
                self.assertEqual(q['metrics']['genes']['median'],2)
                self.assertEqual(q['metrics']['mito']['median'],10)
                self.assertEqual(sum(q['metrics']['counts']['counts']),5)
                self.assertEqual(sum(q['metrics']['mito']['counts']),4)
                self.assertEqual(q['donor']['distinct'],2)
                self.assertEqual(q['donor']['missing'],2)
                self.assertEqual(q['donor']['missingRetained'],0)
                self.assertEqual({d['label']:d['retained'] for d in q['donor']['groups']},{'D1':1,'D2':0})
                json.dumps(q,allow_nan=False)

    def test_exact_donor_inventory_flags_truncation_and_cardinality(self):
        for labels, complete in [(['D1','D2'], True), (['x'*201,'D2'], False), ([f'D{i}' for i in range(31)], False)]:
            data=ad.AnnData(np.ones((len(labels),2)),obs=pd.DataFrame({'subject':labels},index=[f'c{i}' for i in range(len(labels))]))
            report=quality_control(data,{'donorColumn':'subject','minGenes':0})
            self.assertEqual(report['donor']['inventoryComplete'],complete)
            if complete:
                self.assertEqual(sum(d['retained'] for d in report['donor']['groups'])+report['donor']['missingRetained'],report['retained'])

    def test_noncanonical_sparse_entries_are_checked_before_duplicate_summing(self):
        for values, reason in (([1,-1,4], 'negative'),([0.5,0.5,4], 'fractional')):
            matrix=sparse.csr_matrix((np.array(values,dtype=float),np.array([0,0,1]),np.array([0,3])),shape=(1,4))
            data=ad.AnnData(matrix,var=pd.DataFrame(index=['MT-A','B','C','D']))
            result=quality_control(data,{'minGenes':0})
            self.assertEqual(result['status'],'invalid_counts')
            self.assertGreater(result['invalidValues'][reason],0)

    def test_invalid_expression_never_produces_qc_passes(self):
        for value in (float('nan'),float('inf'),-1,0.25):
            data=self.make_data(); data.X[0,0]=value
            q=quality_control(data,{})
            self.assertEqual(q['status'],'invalid_counts')
            self.assertIsNone(q['retained'])
            self.assertEqual(q['metrics'],{})

    def test_missing_mito_features_and_zero_libraries_are_not_zero_percent(self):
        data=self.make_data();data.var_names=['A','B','C','D']
        q=quality_control(data,{'minGenes':0})
        self.assertEqual(q['status'],'partial')
        self.assertNotIn('mito',q['metrics'])
        self.assertEqual(q['failures']['highMito'],0)
        data.X[:]=0
        q=quality_control(data,{})
        self.assertEqual(q['retained'],0)
        self.assertTrue(all(e>=0 for e in q['metrics']['counts']['edges']))

    def test_explicit_layer_and_gene_symbol_mapping(self):
        data=self.make_data()
        data.var['symbols']=data.var_names
        data.var_names=['id1','id2','id3','id4']
        data.layers['counts']=data.X.copy()
        data.X=np.log1p(data.X)
        q=quality_control(data,{'matrix':'layer:counts','geneColumn':'symbols','mitoPrefix':'mt-','minGenes':0})
        self.assertEqual(q['mitoFeatures'],1)
        self.assertEqual(q['status'],'ready')
        self.assertEqual(quality_control(data,{})['status'],'invalid_counts')

    def test_bad_settings_columns_and_changed_source_fail(self):
        for options in ({'minGenes':-1},{'minGenes':5,'maxGenes':4},{'maxMitoPct':101},{'minGenes':None},{'mitoPrefix':''},{'extra':True}):
            with self.assertRaises(ValueError): qc_options(options)
        data=self.make_data()
        for options in ({'donorColumn':'missing'},{'geneColumn':'missing'},{'matrix':'layer:missing'}):
            with self.assertRaises(ValueError):quality_control(data,options)
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'fixture.synthetic.h5ad';data.write_h5ad(path)
            with self.assertRaisesRegex(ValueError,'file_changed'):inspect(path,expected_sha='0'*64)

    def test_donor_identity_is_not_inferred_or_silently_normalized(self):
        data=self.make_data();data.obs['subject']=['D1',' D1','D1 ',None,'']
        q=quality_control(data,{'donorColumn':'subject','minGenes':0,'maxMitoPct':None})
        self.assertEqual(q['donor']['distinct'],3)
        self.assertFalse(q['donor']['identityVerified'])
        self.assertEqual(quality_control(data,{})['donor']['column'],'')

    @unittest.skipUnless(os.environ.get('CAUSALMED_TEST_RAW_H5AD'),'Set public raw PBMC3k path')
    def test_public_raw_counts_match_independent_full_matrix_calculation(self):
        path=os.environ['CAUSALMED_TEST_RAW_H5AD'];report=inspect(path)
        self.assertEqual((report['cells'],report['genes']),(2700,32738))
        data=ad.read_h5ad(path)
        totals=np.asarray(data.X.sum(axis=1)).ravel()
        genes=np.asarray((data.X>0).sum(axis=1)).ravel()
        mask=data.var_names.str.upper().str.startswith('MT-')
        mito=np.asarray(data.X[:,mask].sum(axis=1)).ravel()/totals*100
        passed=(totals>0)&(genes>=200)&(genes<=6000)&(mito<=20)
        q=report['qc']
        self.assertEqual(q['status'],'ready')
        self.assertEqual(q['retained'],int(passed.sum()))
        self.assertAlmostEqual(q['metrics']['mito']['median'],float(np.median(mito)),places=5)

    @unittest.skipUnless(os.environ.get('CAUSALMED_TEST_DONOR_H5AD'),'Set public Kang2018 path')
    def test_public_donor_mapping_uses_actual_replicate_annotation(self):
        report=inspect(os.environ['CAUSALMED_TEST_DONOR_H5AD'],options={'donorColumn':'replicate'})
        q=report['qc']
        self.assertEqual(report['cells'],24673)
        self.assertEqual(q['donor']['distinct'],8)
        self.assertEqual(q['donor']['missing'],0)
        self.assertEqual(sum(d['cells'] for d in q['donor']['groups']),24673)
        self.assertEqual(sum(d['retained'] for d in q['donor']['groups']),q['retained'])
        self.assertEqual(q['status'],'partial')

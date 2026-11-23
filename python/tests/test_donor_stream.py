"""Synthetic donor fixtures verify software behavior, never biological identity."""
import hashlib
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import anndata as ad
import h5py
import numpy as np
import pandas as pd
from scipy import sparse
from causalmed_worker.backed import H5Frame, open_research_h5ad
from causalmed_worker.qc import donor_selection, donor_summary, quality_control
from causalmed_worker.h5ad import inspect


def reference(series, passed, valid=True):
    labels=series.astype('string')
    absent=labels.isna() | labels.str.strip().eq('').fillna(True)
    labels=labels.mask(absent)
    counts=labels.value_counts(sort=False).sort_values(ascending=False,kind='stable')
    retained=labels.iloc[passed].value_counts() if valid else None
    return {'distinct':len(counts),'missing':int(absent.sum()),
            'missingRetained':int((absent.to_numpy(dtype=bool)&passed).sum()) if valid else None,
            'otherCells':int(counts.iloc[30:].sum()),
            'inventoryComplete':len(counts)<=30 and all(len(str(k).encode('utf-8'))<=200 for k in counts.index),
            'groups':[{'label':str(k)[:200],'cells':int(v),'retained':int(retained.get(k,0)) if valid else None} for k,v in counts.iloc[:30].items()]}


class DonorStreamTests(unittest.TestCase):
    def test_typed_missing_whitespace_identity_and_raw_arrays_across_blocks(self):
        columns={
            'text':pd.array(['D1',' D1','D1 ',None,'','\t','\u2003','中文','😀','x'*201],dtype='string'),
            'category':pd.Categorical(['B','A',None,'B','A','C',' C','\u2003','A','B'],
                                       categories=['unused',' C','C','A','B','\u2003'],ordered=True),
            'integer':pd.array([1,2,None,1,3,2,3,1,2,3],dtype='Int64'),
            'boolean':pd.array([True,False,None,True,False,False,True,False,True,False],dtype='boolean'),
            'float':[1.,np.nan,2.,1.,3.,3.,2.,1.,3.,2.],
        }
        frame=pd.DataFrame(columns,index=[f'c{i}' for i in range(10)])
        mask=np.array([True,False,True,False,True,False,True,False,True,False])
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'typed.synthetic.h5ad'
            with ad.settings.override(allow_write_nullable_strings=True):
                ad.AnnData(np.ones((10,1)),obs=frame).write_h5ad(path,convert_strings_to_categoricals=False)
            data=open_research_h5ad(path)
            try:
                with patch.object(H5Frame,'__getitem__',side_effect=AssertionError('full column decode')):
                    for key in frame:
                        for size in [1,3,8192]:
                            for valid in [True,False]:
                                with self.subTest(key=key,size=size,valid=valid):
                                    self.assertEqual(donor_summary(data.obs,key,mask,valid,size),reference(frame[key],mask,valid))
                            ids,missing=donor_selection(data.obs,key,size)
                            raw=frame[key].astype('string').fillna('').to_numpy(dtype=str)
                            np.testing.assert_array_equal(ids,raw)
                            np.testing.assert_array_equal(missing,np.array([not value.strip() for value in raw]))
            finally:
                data.file.close()

    def test_high_cardinality_all_missing_empty_and_mask_validation(self):
        frames=[pd.DataFrame({'d':[f'D{i%1301}' for i in range(17003)]}),
                pd.DataFrame({'d':pd.array([None,'',' ',None],dtype='string')}),
                pd.DataFrame({'d':pd.array([],dtype='string')})]
        for frame in frames:
            mask=np.arange(len(frame))%3==0
            for size in [997,8192]:
                self.assertEqual(donor_summary(frame,'d',mask,True,size),reference(frame.d,mask))
            with self.assertRaisesRegex(ValueError,'invalid_h5ad'):
                donor_summary(frame,'d',np.ones(len(frame)+1,dtype=bool))

    def test_modern_dense_csr_csc_qc_callback_without_full_donor_decode(self):
        for matrix in [np.asarray,sparse.csr_matrix,sparse.csc_matrix]:
            with self.subTest(matrix=matrix.__name__),tempfile.TemporaryDirectory() as directory:
                source=Path(directory)/'qc.synthetic.h5ad'
                blank = '\t' * 8
                obs=pd.DataFrame({'donor':pd.Categorical(
                    ['D1', None, ' D1', 'D1 ', blank],
                    categories=['D1', ' D1', 'D1 ', blank, 'unused-long-category'],
                )},
                                 index=['a','b','c','d','e'])
                original=ad.AnnData(matrix([[1,2],[0,0],[3,4],[0,2],[3,4]]),obs=obs,var=pd.DataFrame(index=['MT-A','B']))
                original.write_h5ad(source)
                data=open_research_h5ad(source);saved={}
                options={'donorColumn':'donor','minGenes':0,'maxMitoPct':None}
                try:
                    with patch.object(H5Frame,'__getitem__',side_effect=AssertionError('full column decode')):
                        result=quality_control(data,options,selection_callback=saved.update)
                finally:
                    data.file.close()
                self.assertEqual(result['donor'],quality_control(original,options)['donor'])
                np.testing.assert_array_equal(saved['retained'],[True,False,True,True,True])
                np.testing.assert_array_equal(saved['donor_id'], ['D1', '', ' D1', 'D1 ', blank])
                self.assertEqual(saved['donor_id'].dtype, np.dtype('U8'))
                np.testing.assert_array_equal(saved['donor_missing'],[False,True,False,False,True])
                self.assertEqual(result['donor']['missingRetained'],1)
                self.assertFalse(result['donor']['identityVerified'])

    def test_invalid_expression_reports_donor_totals_without_retention_or_snapshot(self):
        data=ad.AnnData(np.array([[np.nan],[1.],[2.]]),obs=pd.DataFrame({'donor':['A','B',None]},index=['a','b','c']))
        saved=[]
        result=quality_control(data,{'donorColumn':'donor'},selection_callback=saved.append)
        self.assertEqual(result['status'],'invalid_counts')
        self.assertEqual(result['donor']['distinct'],2)
        self.assertEqual(result['donor']['missing'],1)
        self.assertIsNone(result['donor']['missingRetained'])
        self.assertTrue(all(row['retained'] is None for row in result['donor']['groups']))
        self.assertEqual(saved,[])

    def test_late_corrupt_category_aborts_summary_and_snapshot(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'corrupt.synthetic.h5ad'
            data=ad.AnnData(np.ones((3,1)),obs=pd.DataFrame({'donor':pd.Categorical([None,'A','B'])},index=['a','b','c']))
            data.write_h5ad(path)
            with h5py.File(path,'r+') as handle:
                handle['obs/donor/codes'][2]=100
            data=open_research_h5ad(path)
            try:
                for operation,args in [(donor_summary,(np.ones(3,dtype=bool),True,1)),(donor_selection,(1,))]:
                    with self.assertRaises(ValueError): operation(data.obs,'donor',*args)
            finally:
                data.file.close()

    @unittest.skipUnless(os.environ.get('CAUSALMED_TEST_DONOR_H5AD'),'Set public Kang path')
    def test_public_donor_census_and_saved_raw_ids_match_independent_mask(self):
        source=Path(os.environ['CAUSALMED_TEST_DONOR_H5AD'])
        before=hashlib.sha256(source.read_bytes()).hexdigest()
        original=ad.read_h5ad(source)
        totals=np.asarray(original.X.sum(axis=1)).ravel()
        genes=np.asarray((original.X>0).sum(axis=1)).ravel()
        passed=(totals>0)&(genes>=200)&(genes<=6000)
        with tempfile.TemporaryDirectory() as directory:
            report=inspect(source,options={'donorColumn':'replicate'},artifact_dir=directory)
            expected=reference(original.obs.replicate,passed)
            self.assertEqual({k:report['qc']['donor'][k] for k in expected},expected)
            self.assertEqual(report['qc']['retained'],24562)
            with np.load(Path(directory)/'cells.npz',allow_pickle=False) as saved:
                np.testing.assert_array_equal(saved['retained'],passed)
                np.testing.assert_array_equal(saved['donor_id'],original.obs.replicate.astype('string').fillna('').to_numpy(dtype=str))
                np.testing.assert_array_equal(saved['donor_missing'],
                                              original.obs.replicate.astype('string').fillna('').str.strip().eq(''))
        self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(),before)

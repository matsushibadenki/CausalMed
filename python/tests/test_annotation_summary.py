"""Synthetic annotation contracts, not biomedical observations."""
import os
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import anndata as ad
import numpy as np
import pandas as pd
from causalmed_worker.backed import (H5Frame, annotation_group, annotation_schema,
                                    open_research_h5ad)


def reference(series):
    series = series.astype('string')
    counts = series.dropna().value_counts(sort=False).sort_values(ascending=False, kind='stable')
    return {'distinct': len(counts), 'missing': int(series.isna().sum()),
            'complete': len(counts) <= 20 and all(len(str(k).encode('utf-8')) <= 200 for k in counts.index),
            'other': int(counts.iloc[20:].sum()),
            'counts': [{'label': str(k)[:200], 'count': int(v)} for k,v in counts.iloc[:20].items()]}


class AnnotationSummaryTests(unittest.TestCase):
    def test_types_ties_missing_unicode_and_cross_block_counts(self):
        columns = {
            'condition': pd.Categorical(['B','A',None,'B','C','A','C'], categories=['unused','C','A','B'], ordered=True),
            'number': pd.array([1,2,None,1,3,2,3], dtype='Int64'),
            'flag': pd.array([True,False,None,True,False,True,False], dtype='boolean'),
            'text': pd.array(['日本語','中文',None,'日本語','x'*201,'中文','x'*201], dtype='string'),
            'float': np.array([1,2,np.nan,1,3,2,3], dtype=float),
        }
        frame = pd.DataFrame(columns, index=[f'c{i}' for i in range(7)])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'typed.synthetic.h5ad'
            with ad.settings.override(allow_write_nullable_strings=True):
                ad.AnnData(np.ones((7,1)), obs=frame).write_h5ad(path, convert_strings_to_categoricals=False)
            data = open_research_h5ad(path)
            try:
                # Modern schema/group paths must never request full decoded columns.
                with patch.object(H5Frame, '__getitem__', side_effect=AssertionError('full column read')):
                    for key in frame:
                        for size in [1,2,4,8192]:
                            with self.subTest(key=key, size=size):
                                self.assertEqual(annotation_group(data.obs,key,size),reference(frame[key]))
                                schema=annotation_schema(data.obs,key,size)
                                self.assertTrue(schema['nullable'])
                                self.assertEqual(schema['dtype'],str(frame[key].dtype))
                self.assertEqual(annotation_schema(data.obs,'condition',2)['kind'],'categorical')
            finally:
                data.file.close()

    def test_high_cardinality_exact_top_other_and_all_missing(self):
        frame = pd.DataFrame({'x':[f'value-{i%1301}' for i in range(15003)],
                              'missing':pd.array([None]*15003,dtype='Int64')})
        for key in frame:
            for size in [8192,1000]:
                self.assertEqual(annotation_group(frame,key,size),reference(frame[key]))
        self.assertFalse(annotation_group(frame,'x')['complete'])
        self.assertEqual(annotation_group(frame,'missing')['counts'],[])

    def test_corrupt_late_rows_are_not_skipped_after_a_missing_block(self):
        import h5py
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'corrupt.synthetic.h5ad'
            frame=pd.DataFrame({'condition':pd.Categorical([None,'A','B'])},index=['a','b','c'])
            ad.AnnData(np.ones((3,1)),obs=frame).write_h5ad(path)
            with h5py.File(path,'r+') as handle:
                handle['obs/condition/codes'][2]=100
            data=open_research_h5ad(path)
            try:
                for operation in [annotation_schema,annotation_group]:
                    with self.assertRaises(ValueError):
                        operation(data.obs,'condition',1)
            finally:
                data.file.close()

    @unittest.skipUnless(os.environ.get('CAUSALMED_TEST_DONOR_H5AD'), 'Set public Kang path')
    def test_public_kang_summaries_qc_and_retained_export(self):
        from causalmed_worker.h5ad import inspect
        from causalmed_worker.selection import export_selection
        source=Path(os.environ['CAUSALMED_TEST_DONOR_H5AD'])
        before=hashlib.sha256(source.read_bytes()).hexdigest()
        original=ad.read_h5ad(source)
        with tempfile.TemporaryDirectory() as directory:
            run=Path(directory)/'run'
            report=inspect(source,options={'donorColumn':'replicate'},artifact_dir=run)
            for key,group in report['groups'].items():
                self.assertEqual(group,reference(original.obs[key]))
            for schema in report['obsSchema']:
                column=original.obs[schema['name']]
                self.assertEqual(schema['dtype'],str(column.dtype))
                self.assertEqual(schema['nullable'],bool(column.isna().any()))
            self.assertEqual(report['qc']['retained'],24562)
            self.assertEqual(report['qc']['excluded'],111)
            output=Path(directory)/'retained.h5ad'
            export_selection(source,run,output,report['selection'],'export_h5ad')
            child=ad.read_h5ad(output)
            mask=np.load(run/'cells.npz',allow_pickle=False)['retained']
            self.assertEqual(child.shape,(24562,15706))
            self.assertEqual(list(child.obs_names),list(original.obs_names[mask]))
            self.assertEqual((child.X!=original.X[mask]).nnz,0)
            self.assertEqual(list(child.obs.replicate.astype('string')),list(original.obs.replicate.iloc[np.flatnonzero(mask)].astype('string')))
        self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(),before)

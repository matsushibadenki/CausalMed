"""Streamed matrix correctness and explicit numeric block bounds; synthetic QA only."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
try:
    import anndata as ad
    import numpy as np
    import pandas as pd
    from scipy import sparse
except ImportError:
    ad = None
from causalmed_worker.h5ad import inspect
from causalmed_worker.selection import load_selection, export_selection, file_digest
from causalmed_worker.stream_export import stream_retained_h5ad


@unittest.skipIf(ad is None, 'Install python/requirements.txt for H5AD tests')
class StreamExportTests(unittest.TestCase):
    def test_dense_csr_csc_layers_raw_embeddings_pairwise_and_nullable_annotations(self):
        for form in ('dense', 'csr', 'csc'):
            with self.subTest(form=form), tempfile.TemporaryDirectory() as directory:
                p = Path(directory)
                matrix = np.repeat((np.arange(79) % 3)[:, None], 503, axis=1).astype(np.int32)
                x = matrix if form == 'dense' else getattr(sparse, form + '_matrix')(matrix)
                obs = pd.DataFrame({'donor': pd.Categorical(['A','B','C'] * 26 + ['A']),
                                    'nullable': pd.array([None, 3, 5] * 26 + [3], dtype='Int64'),
                                    'flag': pd.array([True, None, False] * 26 + [None], dtype='boolean')}, index=[f'c{i}' for i in range(79)])
                data = ad.AnnData(x, obs=obs, var=pd.DataFrame(index=[f'g{i}' for i in range(503)]))
                data.layers['dense'] = matrix.astype(np.float64) * -0.25
                data.layers['sparse'] = sparse.csc_matrix(matrix)
                data.obsm['dense'] = np.arange(79 * 17).reshape(79, 17)
                data.obsm['frame'] = pd.DataFrame({'label': pd.Categorical(['A']*79), 'value': np.arange(79)}, index=data.obs_names)
                data.obsp['dense'] = np.arange(79*79).reshape(79,79)
                data.obsp['csr'] = sparse.csr_matrix(data.obsp['dense'])
                data.obsp['csc'] = sparse.csc_matrix(data.obsp['dense'])
                data.varm['fixed'] = np.arange(1006).reshape(503,2)
                data.varp['fixed'] = sparse.eye(503,format='csr')
                data.uns['record'] = {'label': 'Synthetic fixture', 'values': np.arange(19)}
                data.raw = ad.AnnData(sparse.csc_matrix(np.ones((79, 509))), obs=data.obs.copy(), var=pd.DataFrame(index=[f'r{i}' for i in range(509)]))
                source = p/'source.h5ad'; data.write_h5ad(source); sha = file_digest(source)
                report = inspect(source, options={'minGenes':1,'minCounts':800,'maxMitoPct':None}, artifact_dir=p/'qc')
                manifest, arrays = load_selection(p/'qc', report['selection']); rows = np.flatnonzero(arrays['retained'])
                events = []
                _, stats = stream_retained_h5ad(source,p/'out.h5ad',manifest,arrays,report['selection'],lambda n,phase:events.append(n),block_bytes=1024)
                self.assertLessEqual(stats['maxReadBlockBytes'],1024)
                self.assertEqual(events,sorted(events)); self.assertGreater(stats['readBlocks'],50)
                actual = ad.read_h5ad(p/'out.h5ad'); expected = data[rows,:].copy()
                dense = lambda v: v.toarray() if sparse.issparse(v) else np.asarray(v)
                np.testing.assert_array_equal(dense(actual.X),dense(expected.X))
                for key in data.layers: np.testing.assert_array_equal(dense(actual.layers[key]),dense(expected.layers[key]))
                for key in data.obsp: np.testing.assert_array_equal(dense(actual.obsp[key]),dense(expected.obsp[key]))
                np.testing.assert_array_equal(actual.obsm['dense'],expected.obsm['dense'])
                pd.testing.assert_frame_equal(actual.obsm['frame'],expected.obsm['frame'])
                pd.testing.assert_series_equal(actual.obs['donor'],data.obs.iloc[rows]['donor'])
                pd.testing.assert_series_equal(actual.obs['nullable'],expected.obs['nullable'])
                pd.testing.assert_series_equal(actual.obs['flag'],expected.obs['flag'])
                np.testing.assert_array_equal(actual.raw.X.toarray(),expected.raw.X.toarray())
                np.testing.assert_array_equal(actual.varm['fixed'],data.varm['fixed'])
                np.testing.assert_array_equal(actual.varp['fixed'].toarray(),data.varp['fixed'].toarray())
                np.testing.assert_array_equal(actual.uns['record']['values'],data.uns['record']['values'])
                self.assertEqual(file_digest(source),sha)

    def test_single_large_sparse_row_is_split_and_export_never_uses_to_memory(self):
        with tempfile.TemporaryDirectory() as directory:
            p=Path(directory); matrix=np.vstack([np.ones(12000),np.zeros(12000),np.full(12000,2)])
            data=ad.AnnData(sparse.csr_matrix(matrix));data.write_h5ad(p/'source.h5ad')
            report=inspect(p/'source.h5ad',options={'minGenes':1,'maxGenes':None,'maxMitoPct':None},artifact_dir=p/'qc')
            manifest, arrays=load_selection(p/'qc',report['selection'])
            _,stats=stream_retained_h5ad(p/'source.h5ad',p/'tiny.h5ad',manifest,arrays,report['selection'],lambda *a:None,1024)
            self.assertLessEqual(stats['maxReadBlockBytes'],1024);self.assertGreater(stats['readBlocks'],500)
            np.testing.assert_array_equal(ad.read_h5ad(p/'tiny.h5ad').X.toarray(),matrix[[0,2]])
            with patch.object(ad.AnnData,'to_memory',side_effect=AssertionError('full matrix materialization forbidden')),patch.object(ad,'read_h5ad',side_effect=AssertionError('backed read can eagerly load layers')):
                result=export_selection(p/'source.h5ad',p/'qc',p/'full.h5ad',report['selection'],'export_h5ad')
            self.assertLessEqual(result['streaming']['maxReadBlockBytes'],result['streaming']['blockBudgetBytes'])

    def test_legacy_raw_dot_layout_is_filtered_instead_of_copied_whole(self):
        import h5py
        with tempfile.TemporaryDirectory() as directory:
            p=Path(directory);x=np.ones((13,17),dtype=np.int32);x[::2]=0
            data=ad.AnnData(sparse.csr_matrix(x));data.raw=ad.AnnData(sparse.csc_matrix(np.arange(13*23).reshape(13,23)))
            data.write_h5ad(p/'source.h5ad');report=inspect(p/'source.h5ad',options={'minGenes':1,'maxMitoPct':None},artifact_dir=p/'qc')
            manifest,arrays=load_selection(p/'qc',report['selection']);rows=np.flatnonzero(arrays['retained'])
            # Change only the on-disk layout after making synthetic decisions.
            with h5py.File(p/'source.h5ad','r+') as f:
                f.move('raw/X','raw.X');f.move('raw/var','raw.var');del f['raw']
            stream_retained_h5ad(p/'source.h5ad',p/'out.h5ad',manifest,arrays,report['selection'],lambda *a:None,1024)
            actual=ad.read_h5ad(p/'out.h5ad')
            np.testing.assert_array_equal(actual.raw.X.toarray(),data.raw.X[rows].toarray())
            self.assertEqual(list(actual.raw.var_names),list(data.raw.var_names))

    def test_unknown_row_encoding_aborts_without_touching_source(self):
        import h5py
        with tempfile.TemporaryDirectory() as directory:
            p=Path(directory);ad.AnnData(np.ones((4,5))).write_h5ad(p/'source.h5ad')
            with h5py.File(p/'source.h5ad','r+') as f:
                g=f['obsm'].create_group('unknown');g.attrs['encoding-type']='unknown';g.attrs['encoding-version']='9'
            # Use a clean QC source to obtain decisions, then point the low-level encoder
            # at this deliberately malformed fixture; it must fail closed.
            ad.AnnData(np.ones((4,5))).write_h5ad(p/'clean.h5ad')
            report=inspect(p/'clean.h5ad',options={'minGenes':1},artifact_dir=p/'qc')
            manifest,arrays=load_selection(p/'qc',report['selection']);before=file_digest(p/'source.h5ad')
            with self.assertRaisesRegex(ValueError,'unsupported_export_encoding'):
                stream_retained_h5ad(p/'source.h5ad',p/'staged.h5ad',manifest,arrays,report['selection'],lambda *a:None)
            self.assertEqual(file_digest(p/'source.h5ad'),before)

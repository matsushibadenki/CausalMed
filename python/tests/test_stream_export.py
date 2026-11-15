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
    def test_legacy_variable_strings_use_projected_bounded_reads(self):
        import h5py
        from anndata.io import write_elem
        from causalmed_worker.backed import LegacyFrame, VLEN_BLOCK_ROWS, read_bounded_strings

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'legacy.h5ad'
            count = 769
            dtype = np.dtype([('cell', h5py.string_dtype('utf-8')),
                              ('text', h5py.string_dtype('utf-8')), ('donor', 'i2')])
            table = np.empty(count, dtype=dtype)
            table['cell'] = [f'細胞-{i // 2}' for i in range(count)]
            table['text'] = ['値' * (100 + i % 7) for i in range(count)]
            table['donor'] = np.arange(count) % 3 - 1
            data = np.tile([1, 4], (count, 1))
            data[::7] = 0
            with h5py.File(source, 'w') as handle:
                handle.create_dataset('obs', data=table)
                handle.create_dataset('var', data=np.array([(b'MT-A',), (b'G1',)], dtype=[('gene', 'S4')]))
                write_elem(handle, 'X', sparse.csc_matrix(data))
                write_elem(handle, 'uns', {'donor_categories': np.array([f'D{i}' for i in range(300)])})
            expected = ad.read_h5ad(source)
            reads = []

            def limited_read(node, selection, **kwargs):
                values = read_bounded_strings(node, selection, **kwargs)
                if kwargs.get('field') in ('text', 'cell'):
                    self.assertLessEqual(len(values), VLEN_BLOCK_ROWS)
                    reads.append(len(values))
                return values

            original_getitem = h5py.Dataset.__getitem__

            def reject_full_table(node, selection, *args, **kwargs):
                if node.name in ('/obs', '/var') and 'new_dtype' not in kwargs:
                    raise AssertionError('legacy table decoded without field projection')
                return original_getitem(node, selection, *args, **kwargs)

            with patch.object(LegacyFrame, 'index', new=property(
                lambda frame: self.fail('full legacy index decode')
            )), patch('causalmed_worker.backed.read_bounded_strings', side_effect=limited_read), patch.object(
                h5py.Dataset, '__getitem__', reject_full_table
            ):
                run = root / 'run'
                report = inspect(source, options={'minGenes': 0, 'donorColumn': 'donor'}, artifact_dir=run)
                output = root / 'result.h5ad'
                export_selection(source, run, output, report['selection'], 'export_h5ad')
            self.assertGreater(len(reads), 3)
            self.assertEqual(report['duplicateCellIds'], int(expected.obs_names.duplicated().sum()))
            actual = ad.read_h5ad(output)
            keep = data.sum(axis=1) > 0
            pd.testing.assert_frame_equal(actual.obs[expected.obs.columns], expected.obs.iloc[keep])
            pd.testing.assert_frame_equal(actual.var, expected.var)
            np.testing.assert_array_equal(actual.X.toarray(), data[keep])

    def test_compact_disk_row_index_preserves_offsets_across_blocks(self):
        from causalmed_worker.stream_export import retained_row_index

        retained = np.zeros(17000, dtype=bool)
        retained[[0, 8191, 8192, 16384, 16999]] = True
        rows = retained_row_index(retained)
        self.assertIsInstance(rows, np.memmap)
        self.assertEqual(rows.dtype, np.dtype('int32'))
        np.testing.assert_array_equal(rows, np.flatnonzero(retained))
        self.assertEqual(len(retained_row_index(~np.ones(10, dtype=bool))), 0)

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

    def test_modern_obs_is_not_materialized_and_original_encoding_is_preserved(self):
        import h5py
        from anndata.io import read_elem
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            obs = pd.DataFrame({
                'label': ['日本語', '细胞', 'English', 'last'],
                'category': pd.Categorical(['A','B',None,'A'],categories=['unused','B','A'],ordered=True),
                'nullable': pd.array([1,None,3,4],dtype='Int64'),
                'flag': pd.array([True,None,False,True],dtype='boolean')},
                index=pd.Index(['dup','dup','c','d'],name='cell-id'))
            data = ad.AnnData(np.array([[2,1],[0,0],[1,2],[3,1]]),obs=obs)
            data.write_h5ad(root/'source.h5ad')
            report = inspect(root/'source.h5ad',options={'minGenes':1,'maxMitoPct':None},artifact_dir=root/'qc')
            manifest, arrays = load_selection(root/'qc',report['selection'])
            arrays['canonical_donor_id'] = np.array(['common-1','','common-2','common-1'])
            arrays['canonical_donor_reviewed'] = np.array([True,False,False,True])
            def guarded(node,*args,**kwargs):
                if node.attrs.get('encoding-type') == 'dataframe':
                    raise AssertionError('Full annotation frame decoded during export')
                return read_elem(node,*args,**kwargs)
            with patch('anndata.io.read_elem',side_effect=guarded):
                provenance, _ = stream_retained_h5ad(root/'source.h5ad',root/'out.h5ad',manifest,arrays,report['selection'],lambda *a:None,1024)
            actual = ad.read_h5ad(root/'out.h5ad')
            rows = np.flatnonzero(arrays['retained'])
            pd.testing.assert_frame_equal(actual.obs[list(obs.columns)],obs.iloc[rows])
            generated = ['source_row','retained','total_counts','detected_genes','mito_percent','donor_id','donor_missing','canonical_donor_id','canonical_donor_reviewed']
            self.assertEqual(list(actual.obs.columns),list(obs.columns)+[provenance+'_'+key for key in generated])
            for key in generated:
                np.testing.assert_array_equal(actual.obs[provenance+'_'+key].to_numpy(),arrays[key][rows])
            with h5py.File(root/'source.h5ad') as original,h5py.File(root/'out.h5ad') as output:
                for name in obs:
                    self.assertEqual(dict(original['obs'][name].attrs),dict(output['obs'][name].attrs))
                self.assertEqual(output['obs'].attrs['_index'],'cell-id')

    def test_invalid_categorical_codes_are_rejected_without_frame_decode(self):
        import h5py
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            data = ad.AnnData(np.ones((3,2)),obs=pd.DataFrame({'label':pd.Categorical(['A','B','A'])},index=['a','b','c']))
            data.write_h5ad(root/'source.h5ad')
            report = inspect(root/'source.h5ad',options={'minGenes':1},artifact_dir=root/'qc')
            manifest,arrays = load_selection(root/'qc',report['selection'])
            for invalid in (-2,2):
                with self.subTest(code=invalid):
                    with h5py.File(root/'source.h5ad','r+') as source:
                        source['obs/label/codes'][0] = invalid
                    before = file_digest(root/'source.h5ad')
                    with self.assertRaisesRegex(ValueError,'unsupported_export_encoding'):
                        stream_retained_h5ad(root/'source.h5ad',root/'out.h5ad',manifest,arrays,report['selection'],lambda *a:None,1024)
                    self.assertEqual(file_digest(root/'source.h5ad'),before)
            with h5py.File(root/'source.h5ad','r+') as source:
                source['obs/label/codes'][0] = 0
                source['obs/label/categories'][1] = 'A'
            with self.assertRaisesRegex(ValueError,'unsupported_export_encoding'):
                stream_retained_h5ad(root/'source.h5ad',root/'out.h5ad',manifest,arrays,report['selection'],lambda *a:None,1024)

    def test_generated_columns_are_selected_and_written_within_budget(self):
        import h5py
        from causalmed_worker.stream_export import H5adStreamCopy
        with tempfile.TemporaryDirectory() as directory:
            rows = np.arange(1,20000,3)
            values = {'count':np.arange(20000,dtype=np.int64),
                      'metric':np.linspace(0,1,20000),'flag':np.arange(20000)%2==0,
                      'donor':np.array(['日本語','细胞','English','']*5000)}
            copier = H5adStreamCopy(1024)
            with h5py.File(Path(directory)/'generated.h5ad','w') as handle:
                for name,value in values.items():
                    copier.generated_column(value,handle,name,rows)
                for name,value in values.items():
                    actual = handle[name].asstr()[:] if value.dtype.kind == 'U' else handle[name][:]
                    np.testing.assert_array_equal(actual,value[rows])
            self.assertLessEqual(copier.max_generated_bytes,1024)
            self.assertGreater(copier.generated_blocks,100)
            with h5py.File(Path(directory)/'invalid.h5ad','w') as handle:
                with self.assertRaisesRegex(ValueError,'unsupported_export_encoding'):
                    copier.generated_column(np.array(['x'*300]),handle,'too-wide',np.array([0]))
                self.assertNotIn('too-wide',handle)

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

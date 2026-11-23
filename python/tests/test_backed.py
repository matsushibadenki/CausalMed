"""Synthetic fixtures verify reads, not biomedical conclusions."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import PropertyMock, patch

try:
    import anndata as ad
    import h5py
    import numpy as np
    import pandas as pd
    from scipy import sparse
except ImportError:
    ad = None

from causalmed_worker.backed import open_research_h5ad
from causalmed_worker.h5ad import inspect
from causalmed_worker.qc import quality_control


@unittest.skipIf(ad is None, 'Install python/requirements.txt')
class BackedTests(unittest.TestCase):
    def test_unused_payloads_are_not_read_and_selected_layers_remain_backed(self):
        for matrix in (np.arange(12).reshape(4, 3), sparse.csr_matrix(np.arange(12).reshape(4, 3)), sparse.csc_matrix(np.arange(12).reshape(4, 3))):
            with self.subTest(matrix=type(matrix).__name__), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / 'synthetic.h5ad'
                data = ad.AnnData(matrix, obs=pd.DataFrame(index=['a','b','c','d']), var=pd.DataFrame(index=['MT-A','B','C']))
                data.layers['counts'] = matrix.copy()
                data.raw = data.copy()
                data.uns['unused'] = np.ones((100, 100))
                data.obsm['unused'] = np.ones((4, 100))
                data.obsp['unused'] = sparse.eye(4, format='csr')
                data.write_h5ad(path)
                original = h5py.Dataset.__getitem__
                reads = []
                def read(node, key):
                    reads.append(node.name)
                    if node.name.startswith(('/uns/unused', '/obsm/', '/obsp/')):
                        raise AssertionError('Unused payload read: ' + node.name)
                    return original(node, key)
                with patch.object(h5py.Dataset, '__getitem__', read):
                    projection = open_research_h5ad(path)
                    try:
                        self.assertFalse(any(name.startswith('/layers/') for name in reads))
                        layer = projection.layers['counts']
                        self.assertFalse(isinstance(layer, np.ndarray) or sparse.issparse(layer))
                        block = layer[1:3, :]
                        np.testing.assert_array_equal(block.toarray() if sparse.issparse(block) else block,
                                                      np.arange(12).reshape(4,3)[1:3])
                    finally:
                        projection.file.close()
                    options = {'matrix':'layer:counts','minGenes':0,'maxGenes':None,'maxMitoPct':None}
                    report = inspect(path, options=options)
                    expected = quality_control(data, options)
                    self.assertEqual({k:v for k,v in report['qc'].items() if k != 'annotationContext'}, expected)
                    self.assertEqual(report['qc']['retained'], 4)
                    raw_report = inspect(path, options={**options, 'matrix':'raw'})
                    self.assertEqual(raw_report['qc']['metrics'], report['qc']['metrics'])
                    self.assertEqual(report['matrices'], ['X', 'raw', 'layer:counts'])

    def test_feature_mask_crosses_blocks_without_full_annotation_decode(self):
        from causalmed_worker.backed import H5Frame
        from causalmed_worker.qc import mitochondrial_mask
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'features.synthetic.h5ad'
            labels = ['OTHER'] * 8195
            labels[8191:8195] = ['mt-A', 'MT-B', None, ' MT-C']
            names = [f'g{i}' for i in range(len(labels))]
            names[-1] = 'mt-index'
            var = pd.DataFrame({'symbol': pd.Categorical(labels)}, index=names)
            data = ad.AnnData(np.ones((1, len(labels))), var=var)
            data.raw = data.copy()
            data.layers['counts'] = data.X.copy()
            data.write_h5ad(path)
            projection = open_research_h5ad(path)
            try:
                with patch.object(H5Frame, '__getitem__', side_effect=AssertionError('full feature decode')):
                    for matrix in ('X', 'raw', 'layer:counts'):
                        report = quality_control(projection, {
                            'matrix': matrix, 'geneColumn': 'symbol',
                            'minGenes': 0, 'maxGenes': None,
                        })
                        self.assertEqual(report['mitoFeatures'], 2)
                        self.assertEqual(report['retained'], 1)
                    mask = mitochondrial_mask(projection.var, '_index', 'mt-')
                    np.testing.assert_array_equal(np.flatnonzero(mask), [8194])
            finally:
                projection.file.close()

    def test_grouped_indices_stream_duplicate_counts_with_missing_values(self):
        from anndata.io import write_elem
        from causalmed_worker.backed import H5Frame, duplicate_frame_ids

        fixtures = [
            pd.array([2**63 + 1, None, 2**63 + 2] * 3000, dtype='UInt64'),
            pd.array([True, None, False] * 3000, dtype='boolean'),
            pd.array(['', None, '細胞'] * 3000, dtype='string'),
            pd.Categorical.from_codes([1, -1, 0] * 3000, ['基準', '摂動']),
            pd.Categorical.from_codes([1, -1, 0] * 3000, [-0.0, 1.5]),
        ]
        with tempfile.TemporaryDirectory() as directory:
            with h5py.File(Path(directory) / 'grouped.h5', 'w') as handle:
                for ordinal, values in enumerate(fixtures):
                    group = handle.create_group(str(ordinal))
                    group.attrs['_index'] = 'ids'
                    group.attrs['column-order'] = []
                    with ad.settings.override(allow_write_nullable_strings=True):
                        write_elem(group, 'ids', values)
                    expected = int(pd.Index(values).duplicated().sum())
                    with patch.object(H5Frame, 'index', new_callable=PropertyMock,
                                      side_effect=AssertionError('full index read')):
                        self.assertEqual(duplicate_frame_ids(H5Frame(group)), expected)

    def test_category_validation_budgets_long_labels_and_checks_utf8(self):
        from causalmed_worker.backed import CategoryDictionary, read_bounded_vlen

        prefix = 'a' + '語' * 23000
        with tempfile.TemporaryDirectory() as directory:
            with h5py.File(Path(directory) / 'long-labels.h5', 'w') as handle:
                labels = handle.create_dataset('labels', data=[prefix + '1', prefix + '2', prefix + '3'],
                                               dtype=h5py.string_dtype())
                original = handle.id.get_mdc_config()
                dictionary = CategoryDictionary(labels)
                read_labels = dictionary._validation_values

                def read_with_cache_check(selection):
                    self.assertLessEqual(handle.id.get_mdc_config().max_size, 1024 * 1024)
                    return read_labels(selection)

                with patch('causalmed_worker.backed.ANNOTATION_SUMMARY_BYTES', 1024), patch.object(
                        dictionary, '_validation_values', side_effect=read_with_cache_check):
                    dictionary.validate()
                    self.assertEqual(handle.id.get_mdc_config().max_size, original.max_size)
                    # A forced digest collision must not merge different labels.
                    with patch('hashlib.sha256') as digest:
                        digest.return_value.digest.return_value = b'collision'
                        CategoryDictionary(labels).validate()
                        labels[2] = prefix + '2'
                        with patch('causalmed_worker.backed.read_bounded_vlen', wraps=read_bounded_vlen) as read:
                            with self.assertRaisesRegex(ValueError, 'invalid_h5ad'):
                                CategoryDictionary(labels).validate()
                        single_reads = [call.args[1].start for call in read.call_args_list
                                        if call.args[1].stop - call.args[1].start == 1]
                        self.assertEqual(single_reads, [0, 1, 2])
                        self.assertEqual(read.call_count, 3)
                        labels[2] = prefix + '3'
                    labels[1] = prefix + '1'
                    with self.assertRaisesRegex(ValueError, 'invalid_h5ad'):
                        CategoryDictionary(labels).validate()
                    labels[1] = prefix.encode() + b'\xc3'
                    with self.assertRaises(UnicodeDecodeError):
                        dictionary.validate()
                    restored = handle.id.get_mdc_config()
                    self.assertEqual((restored.min_size, restored.max_size),
                                     (original.min_size, original.max_size))

    def test_category_validation_shrinks_reads_after_later_long_label(self):
        from causalmed_worker.backed import CategoryDictionary

        with tempfile.TemporaryDirectory() as directory:
            with h5py.File(Path(directory) / 'mixed-labels.h5', 'w') as handle:
                labels = handle.create_dataset(
                    'labels', data=['a', '語' * 25000, '文' * 25000, '字' * 25000],
                    dtype=h5py.string_dtype(),
                )
                dictionary = CategoryDictionary(labels)
                with patch('causalmed_worker.backed.ANNOTATION_SUMMARY_BYTES', 1024), patch.object(
                        dictionary, '_validation_values', wraps=dictionary._validation_values) as read:
                    dictionary.validate()
                slices = [(call.args[0].start, call.args[0].stop) for call in read.call_args_list]
                self.assertEqual(slices, [(0, 1), (1, 4), (1, 2), (2, 3), (3, 4)])

    def test_categorical_index_counts_codes_and_rejects_invalid_dictionaries(self):
        from anndata.io import write_elem
        from causalmed_worker.backed import H5Frame, duplicate_frame_ids, frame_id_stats

        values = pd.Categorical.from_codes([2, -1, 0, 2, -1] * 1800, ['a', 'unused-long-label', 'b'])
        with tempfile.TemporaryDirectory() as directory:
            with h5py.File(Path(directory) / 'codes.h5', 'w') as handle:
                handle.attrs['_index'] = 'ids'
                handle.attrs['column-order'] = []
                write_elem(handle, 'ids', values)
                with patch('causalmed_worker.backed.iter_frame_index',
                           side_effect=AssertionError('categorical labels expanded')):
                    self.assertEqual(duplicate_frame_ids(H5Frame(handle)),
                                     int(pd.Index(values).duplicated().sum()))
                    self.assertEqual(frame_id_stats(H5Frame(handle), measure_width=True),
                                     (int(pd.Index(values).duplicated().sum()), 3))
                handle['ids/codes'][-1] = 3
                with self.assertRaisesRegex(ValueError, 'invalid_h5ad'):
                    duplicate_frame_ids(H5Frame(handle))
                handle['ids/codes'][-1] = -1
                handle['ids/categories'][1] = 'a'
                with self.assertRaisesRegex(ValueError, 'invalid_h5ad'):
                    duplicate_frame_ids(H5Frame(handle))

    def test_numeric_and_nullable_category_index_width_matches_row_projection(self):
        from anndata.io import write_elem
        from causalmed_worker.backed import (
            H5Frame, frame_id_stats, index_matches, iter_frame_index, text_block_width,
        )

        for categories in (np.array([0.1, 0.2, 0.3], dtype=np.float32),
                           pd.array([1, 2, 30000], dtype='Int64'),
                           pd.array(['a', '字', 'unused-long'], dtype='string'),
                           pd.array([True, False], dtype='boolean')):
            for codes in ([1, -1, 0, 1], [-1] * 8192 + [0, 1]):
                with self.subTest(dtype=categories.dtype, rows=len(codes)), tempfile.TemporaryDirectory() as directory:
                    with h5py.File(Path(directory) / 'categories.h5', 'w') as handle:
                        handle.attrs['_index'] = 'ids'
                        handle.attrs['column-order'] = []
                        values = pd.Categorical.from_codes(codes, categories=categories)
                        with ad.settings.override(allow_write_nullable_strings=True):
                            write_elem(handle, 'ids', values)
                        frame = H5Frame(handle)
                        expected = max(text_block_width(block) for _, block in iter_frame_index(frame))
                        saved = np.concatenate([
                            np.asarray(block, dtype=f'U{expected}') for _, block in iter_frame_index(frame)
                        ])
                        self.assertTrue(index_matches(frame, saved))
                        saved[-1] = 'wrong'
                        self.assertFalse(index_matches(frame, saved))
                        with patch('causalmed_worker.backed.iter_frame_index',
                                   side_effect=AssertionError('expanded categorical rows')):
                            self.assertEqual(frame_id_stats(frame, measure_width=True),
                                             (int(pd.Index(values).duplicated().sum()), expected))

    def test_string_index_width_reuses_duplicate_scan(self):
        from causalmed_worker.backed import H5Frame, frame_id_stats

        values = ['細胞', 'x', '長い😀識別子', '細胞']
        with tempfile.TemporaryDirectory() as directory:
            with h5py.File(Path(directory) / 'indices.h5', 'w') as handle:
                handle.attrs['_index'] = 'ids'
                handle.attrs['column-order'] = []
                handle.create_dataset('ids', data=values, dtype=h5py.string_dtype('utf-8'))
                self.assertEqual(frame_id_stats(H5Frame(handle), measure_width=True), (1, 6))

    def test_streamed_numeric_indices_match_pandas_duplicate_counts(self):
        from causalmed_worker.backed import H5Frame, LegacyFrame, duplicate_frame_ids

        floats = np.arange(9000, dtype=np.float64)
        floats[[0, 8192]] = [0.0, -0.0]
        floats.view(np.uint64)[[1, 8193]] = [0x7ff8000000000001, 0xfff8000000000002]
        integers = np.arange(9000, dtype=np.uint64) + 2**63
        integers[-1] = integers[0]
        complex_ids = np.arange(9000, dtype=np.complex128)
        complex_ids[[0, 8192]] = [complex(0, -0.0), 0j]
        complex_ids[[1, 8193]] = [complex(1, np.nan), complex(1, -np.nan)]
        complex_ids[8194] = complex(2, np.nan)
        complex_ids[[3, 8195]] = [complex(np.nan, 3), complex(-np.nan, 3)]
        with tempfile.TemporaryDirectory() as directory:
            with h5py.File(Path(directory) / 'indices.h5', 'w') as handle:
                for values in (floats, integers, complex_ids):
                    group = handle.create_group(values.dtype.name)
                    group.attrs['_index'] = 'ids'
                    group.attrs['column-order'] = []
                    group['ids'] = values
                    records = np.empty(len(values), dtype=[('index', values.dtype)])
                    records['index'] = values
                    legacy = handle.create_dataset(values.dtype.name + '-legacy', data=records)
                    expected = int(pd.Index(values).duplicated().sum())
                    for frame in (H5Frame(group), LegacyFrame(legacy)):
                        with patch.object(type(frame), 'index', new_callable=PropertyMock,
                                          side_effect=AssertionError('full index read')):
                            self.assertEqual(duplicate_frame_ids(frame), expected)

    def test_streamed_feature_index_and_large_string_dictionary(self):
        from causalmed_worker.backed import annotation_group, duplicate_frame_ids
        from causalmed_worker.qc import mitochondrial_mask
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'dictionary.synthetic.h5ad'
            categories = [f'gene-{i}' for i in range(8300)]
            categories[-1] = 'MT-last'
            symbols = pd.Categorical.from_codes([8299, -1, 0, 8299], categories, ordered=True)
            var = pd.DataFrame({'symbol': symbols}, index=['mt-先頭', 'g', 'g', 'MT-末尾'])
            ad.AnnData(np.ones((1, 4)), var=var).write_h5ad(path)
            original = h5py.Dataset.__getitem__

            def read(node, key):
                if node.name in ('/var/_index', '/var/symbol/categories'):
                    if (isinstance(key, tuple) and not key) or (isinstance(key, slice) and key == slice(None)):
                        raise AssertionError('full feature index or dictionary decode')
                return original(node, key)

            projection = open_research_h5ad(path)
            try:
                with patch.object(h5py.Dataset, '__getitem__', read):
                    self.assertEqual(duplicate_frame_ids(projection.var), 1)
                    np.testing.assert_array_equal(
                        mitochondrial_mask(projection.var, '_index', 'mt-'),
                        [True, False, False, True],
                    )
                    np.testing.assert_array_equal(
                        mitochondrial_mask(projection.var, 'symbol', 'MT-'),
                        [True, False, False, True],
                    )
                    self.assertEqual(annotation_group(projection.var, 'symbol')['counts'], [
                        {'label': 'MT-last', 'count': 2}, {'label': 'gene-0', 'count': 1},
                    ])
                    self.assertNotIn('index', vars(projection.var))
            finally:
                projection.file.close()

            with h5py.File(path, 'r+') as handle:
                handle['var/symbol/categories'][8193] = 'gene-0'
            projection = open_research_h5ad(path)
            try:
                with self.assertRaisesRegex(ValueError, 'invalid_h5ad'):
                    mitochondrial_mask(projection.var, 'symbol', 'MT-')
            finally:
                projection.file.close()

    def test_vlen_allocation_budget_applies_to_plain_and_compound_reads(self):
        from causalmed_worker.backed import read_bounded_vlen, split_vlen_batch
        with tempfile.TemporaryDirectory() as directory:
            with h5py.File(Path(directory) / 'vlen.h5', 'w') as handle:
                dtype = h5py.string_dtype('utf-8')
                values = ['字' * 20, '😀' * 20, 'short']
                plain = handle.create_dataset('plain', data=values, dtype=dtype)
                compound = handle.create_dataset('compound', data=np.array(
                    list(zip(values, range(3))), dtype=[('text', dtype), ('number', 'i4')]))
                scalar = handle.create_dataset('scalar', data=values[1], dtype=dtype)
                for node, kwargs in [(plain, {}), (compound, {'field': 'text'})]:
                    with self.subTest(node=node.name):
                        with self.assertRaisesRegex(ValueError, 'vlen_budget_exceeded'):
                            read_bounded_vlen(node, slice(None), byte_limit=100, **kwargs)
                        np.testing.assert_array_equal(
                            read_bounded_vlen(node, np.array([0, 2]), byte_limit=100, **kwargs),
                            [values[0], values[2]],
                        )
                batches = list(split_vlen_batch(
                    0, 3, lambda start, stop: read_bounded_vlen(
                        plain, slice(start, stop), byte_limit=100
                    )
                ))
                self.assertEqual([offset for offset, _ in batches], [0, 1])
                np.testing.assert_array_equal(np.concatenate([block for _, block in batches]), values)
                with self.assertRaisesRegex(ValueError, 'vlen_budget_exceeded'):
                    read_bounded_vlen(scalar, (), byte_limit=80)
                self.assertEqual(read_bounded_vlen(scalar, (), byte_limit=81), values[1])

    def test_nullable_category_dictionaries_project_preserve_values_and_reject_missing(self):
        from causalmed_worker.backed import annotation_group, iter_annotation_column

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'nullable.h5ad'
            var = pd.DataFrame(index=['a', 'b', 'c', 'd'])
            for name, labels in [
                ('integer', pd.array([2**63 + 3, 7, 13], dtype='UInt64')),
                ('boolean', pd.array([True, False], dtype='boolean')),
                ('text', pd.array(['中文', '😀', 'unused'], dtype='string')),
            ]:
                var[name] = pd.Categorical.from_codes([0, 1, -1, 0], categories=labels, ordered=True)
            with ad.settings.override(allow_write_nullable_strings=True):
                ad.AnnData(np.ones((1, 4)), var=var).write_h5ad(path, convert_strings_to_categoricals=False)
            from anndata.io import write_elem
            with h5py.File(path, 'r+') as handle, ad.settings.override(allow_write_nullable_strings=True):
                for key in var:
                    group = handle['var'][key]
                    del group['categories']
                    write_elem(group, 'categories', var[key].cat.categories.array)
            projection = open_research_h5ad(path)
            try:
                for key in var:
                    with self.subTest(key=key):
                        values = pd.concat([block.astype('string') for _, block in
                                            iter_annotation_column(projection.var, key, 2)])
                        pd.testing.assert_series_equal(values, var[key].astype('string'))
                        self.assertEqual(annotation_group(projection.var, key)['missing'], 1)
            finally:
                projection.file.close()
            with h5py.File(path, 'r+') as handle:
                handle['var/integer/categories/mask'][2] = True
            projection = open_research_h5ad(path)
            try:
                with self.assertRaisesRegex(ValueError, 'invalid_h5ad'):
                    annotation_group(projection.var, 'integer')
            finally:
                projection.file.close()

    def test_legacy_raw_and_categorical_codes(self):
        from anndata.io import write_elem
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'legacy.synthetic.h5ad'
            with h5py.File(path, 'w') as handle:
                # Old structured annotation table and dot-separated raw layout.
                handle.create_dataset('obs', data=np.array([(b'c1',0),(b'c2',1),(b'c3',-1)], dtype=[('_index','S2'),('condition','i4')]))
                write_elem(handle, 'var', pd.DataFrame(index=['g']))
                write_elem(handle, 'X', np.ones((3,1)))
                write_elem(handle, 'raw.var', pd.DataFrame(index=['raw-g']))
                write_elem(handle, 'raw.X', sparse.csr_matrix(np.ones((3,1))))
                write_elem(handle, 'uns', {'condition_categories':np.array(['A','B']), 'unrelated_categories':np.ones(10)})
            projection = open_research_h5ad(path)
            try:
                self.assertEqual(list(projection.obs['condition'].cat.categories), ['A','B'])
                self.assertTrue(pd.isna(projection.obs['condition'].iloc[2]))
                self.assertEqual(list(projection.raw.var.index), ['raw-g'])
                self.assertEqual(projection.raw.X.shape, (3,1))
            finally:
                projection.file.close()

    def test_annotation_columns_are_lazy_uncached_and_preserve_nullable_values(self):
        from causalmed_worker.backed import categorical_columns
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'annotations.synthetic.h5ad'
            frame = pd.DataFrame({'unused':np.arange(4),
                                  'condition':pd.Categorical(['A',None,'B','A']),
                                  'nullable':pd.array([1,None,3,4],dtype='Int64')},
                                 index=pd.Index(['dup','dup','c','d'],name='cell'))
            ad.AnnData(np.ones((4,2)),obs=frame).write_h5ad(path)
            original = h5py.Dataset.__getitem__
            reads = []
            def read(node, key):
                reads.append(node.name)
                if node.name == '/obs/unused':
                    raise AssertionError('Unused annotation loaded')
                return original(node,key)
            with patch.object(h5py.Dataset,'__getitem__',read):
                data = open_research_h5ad(path)
                try:
                    self.assertFalse(any(name.startswith('/obs/') and name != '/obs/cell' for name in reads))
                    self.assertEqual(list(categorical_columns(data.obs)),['condition'])
                    pd.testing.assert_series_equal(data.obs['condition'],frame['condition'])
                    pd.testing.assert_series_equal(data.obs['nullable'],frame['nullable'])
                    first = data.obs['condition']
                    first.iloc[0] = 'B'
                    self.assertEqual(data.obs['condition'].iloc[0],'A')
                    self.assertEqual(data.obs.index.duplicated().sum(),1)
                    self.assertEqual(list(data.obs.columns),list(frame.columns))
                finally:
                    data.file.close()

    def test_malformed_annotation_length_fails_when_accessed(self):
        from anndata.io import write_elem
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'malformed.synthetic.h5ad'
            ad.AnnData(np.ones((3,2)),obs=pd.DataFrame({'value':[1,2,3]},index=['a','b','c'])).write_h5ad(path)
            with h5py.File(path,'r+') as handle:
                del handle['obs/value']
                write_elem(handle['obs'],'value',np.array([1,2]))
            data = open_research_h5ad(path)
            try:
                with self.assertRaisesRegex(ValueError,'invalid_h5ad'):
                    data.obs['value']
            finally:
                data.file.close()

    def test_provenance_reads_required_fields_without_decoding_unrelated_payload(self):
        import hashlib
        import json
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'provenance.synthetic.h5ad'
            payload = {'id':'synthetic-origin','sourceSha256':'a'*64,
                       'alignment':{'observation':{'qcId':'synthetic-qc'}}}
            raw = json.dumps(payload)
            data = ad.AnnData(np.ones((3,2)))
            data.uns['causalmed_cohort_probe'] = {'definition_json':raw,
                'definition_sha256':hashlib.sha256(raw.encode()).hexdigest(),
                'retained':3,'unused':np.ones((100,100))}
            data.write_h5ad(path)
            original = h5py.Dataset.__getitem__
            def read(node,key):
                if node.name == '/uns/causalmed_cohort_probe/unused':
                    raise AssertionError('Unrelated provenance payload read')
                return original(node,key)
            with patch.object(h5py.Dataset,'__getitem__',read):
                report = inspect(path)
            self.assertEqual(report['cohortOrigins'][0]['definitionId'],'synthetic-origin')
            self.assertEqual(report['cohortOrigins'][0]['retained'],3)

    def test_column_blocks_preserve_all_supported_annotation_types(self):
        from causalmed_worker.backed import iter_annotation_column
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'blocks.synthetic.h5ad'
            obs = pd.DataFrame({
                'text':['日本語','细胞','English','four','five'],
                'category':pd.Categorical(['A',None,'B','A','B'],categories=['unused','B','A'],ordered=True),
                'integer':pd.array([1,None,3,4,5],dtype='UInt32'),
                'boolean':pd.array([True,None,False,True,False],dtype='boolean'),
                'string':pd.array(['a',None,'b','细胞','日本語'],dtype='string'),
                'numeric':[1.,float('nan'),3.,4.,5.]},index=pd.Index(['dup','dup','c','d','e'],name='cells'))
            with ad.settings.override(allow_write_nullable_strings=True):
                ad.AnnData(np.ones((5,2)),obs=obs).write_h5ad(path)
            expected = ad.read_h5ad(path).obs
            data = open_research_h5ad(path)
            original = h5py.Dataset.__getitem__
            def read(node,key):
                if node.name.startswith('/obs/') and not node.name.endswith('/categories'):
                    self.assertIsInstance(key,slice,node.name)
                    self.assertLessEqual(key.stop-key.start,2,node.name)
                return original(node,key)
            try:
                with patch.object(h5py.Dataset,'__getitem__',read):
                    for name in expected:
                        blocks = list(iter_annotation_column(data.obs,name,2))
                        self.assertEqual([start for start,_ in blocks],[0,2,4])
                        pd.testing.assert_series_equal(pd.concat([block for _,block in blocks]),expected[name])
            finally:
                data.file.close()

    def test_column_blocks_reject_bad_lengths_before_yielding(self):
        from causalmed_worker.backed import iter_annotation_column
        from anndata.io import write_elem
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'bad-blocks.synthetic.h5ad'
            ad.AnnData(np.ones((3,2)),obs=pd.DataFrame({'value':pd.array([1,None,3],dtype='Int64')},index=['a','b','c'])).write_h5ad(path)
            with h5py.File(path,'r+') as handle:
                del handle['obs/value/mask']
                write_elem(handle['obs/value'],'mask',np.array([False,True]))
            data = open_research_h5ad(path)
            try:
                with self.assertRaisesRegex(ValueError,'invalid_h5ad'):
                    next(iter_annotation_column(data.obs,'value',2))
            finally:
                data.file.close()

    def test_embedded_worker_runs_without_importing_project_package(self):
        import json
        import subprocess
        import sys
        modules = ['backed','qc','stream_export','selection','outcome','h5ad']
        source = '\n'.join((Path(__file__).parents[1] / 'causalmed_worker' / (name+'.py')).read_text() for name in modules)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'synthetic.h5ad'
            ad.AnnData(np.ones((2,2))).write_h5ad(path)
            result = subprocess.run(
                [sys.executable, '-I', '-B', '-u', '-', str(path)], input=source,
                capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            self.assertEqual(json.loads(result.stdout.splitlines()[-1])['type'], 'result')

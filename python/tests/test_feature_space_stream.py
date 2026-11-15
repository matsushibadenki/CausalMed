"""Exact compatibility checks for streamed feature identities."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import anndata as ad
import h5py
import numpy as np
import pandas as pd

from causalmed_worker.backed import open_research_h5ad
from causalmed_worker.outcome import match_feature_space


class FeatureSpaceTests(unittest.TestCase):
    def test_numeric_dictionary_and_feature_space_match_full_decode(self):
        for categories in (np.arange(8300, dtype=np.uint64) + np.uint64(2**63),
                           np.arange(8300, dtype=np.float64) / 7):
            with self.subTest(dtype=categories.dtype), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / 'features.h5ad'
                identifiers = pd.Categorical.from_codes([8299, 2, 0], categories, ordered=True)
                var = pd.DataFrame({'feature': identifiers}, index=['😀', '中文', 'a"\\'])
                ad.AnnData(np.ones((1, 3)), var=var).write_h5ad(path)
                source = open_research_h5ad(path)
                original = h5py.Dataset.__getitem__

                def read(node, key):
                    if node.name in ('/var/_index', '/var/feature/categories'):
                        if (isinstance(key, tuple) and not key) or (isinstance(key, slice) and key == slice(None)):
                            raise AssertionError('full label decode')
                    return original(node, key)

                try:
                    with patch.object(h5py.Dataset, '__getitem__', read):
                        for column, values in [('_index', pd.Series(var.index, dtype='string')),
                                               ('feature', var.feature.astype('string'))]:
                            result = {}
                            self.assertEqual(match_feature_space(source.var, column, values.iloc[1], result), 1)
                            expected = json.dumps(sorted(values.tolist()), ensure_ascii=False, separators=(',', ':'))
                            self.assertEqual(result, {
                                'featureCount': 3, 'matchedFeatures': 1,
                                'featureSpaceSha256': hashlib.sha256(expected.encode()).hexdigest(),
                            })
                finally:
                    source.file.close()

                with h5py.File(path, 'r+') as handle:
                    handle['var/feature/categories'][8193] = categories[0]
                source = open_research_h5ad(path)
                try:
                    with self.assertRaisesRegex(ValueError, 'invalid_h5ad'):
                        match_feature_space(source.var, 'feature', 'x', {})
                finally:
                    source.file.close()

    def test_error_precedence_and_exact_count_across_blocks(self):
        for tail, target, error, matched in [
            ('target', 'target', 'feature_ambiguous', 2),
            ('other', 'absent', 'feature_missing', 0),
            ('other', 'target', 'feature_space_invalid', 1),
        ]:
            frame = pd.DataFrame(index=['target', ' '] + [str(i) for i in range(8191)] + [tail])
            result = {}
            with self.assertRaisesRegex(ValueError, error):
                match_feature_space(frame, '_index', target, result)
            self.assertEqual(result, {'featureCount': len(frame), 'matchedFeatures': matched})

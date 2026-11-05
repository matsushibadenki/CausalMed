import csv
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
import zipfile

try:
    import anndata as ad
    import numpy as np
    import pandas as pd
    from scipy import sparse
except ImportError:
    ad = None

from causalmed_worker.h5ad import inspect
from causalmed_worker.selection import export_selection, file_digest, load_selection, compare_selections


@unittest.skipIf(ad is None, 'Install python/requirements.txt for selection tests')
class SelectionTests(unittest.TestCase):
    def make_input(self, directory):
        path = Path(directory) / 'input.synthetic.h5ad'
        data = ad.AnnData(sparse.csr_matrix([[1,4,0],[0,0,0],[4,1,0],[0,2,3]]),
                          obs=pd.DataFrame({'subject': ['A',None,'B','B'], 'existing': [1,2,3,4]}, index=['duplicate','duplicate','c2','c3']),
                          var=pd.DataFrame(index=['MT-A','G1','G2']))
        data.layers['counts'] = data.X.copy()
        data.obsm['embedding'] = np.arange(8).reshape(4,2)
        data.obsp['pairwise'] = sparse.eye(4,format='csr')
        data.raw = data.copy()
        data.uns['original'] = 'preserve me'
        data.write_h5ad(path)
        return path

    def test_history_comparison_uses_cell_identity_and_rejects_tampering(self):
        with tempfile.TemporaryDirectory() as directory:
            source = self.make_input(directory)
            a_dir, b_dir = Path(directory)/'a', Path(directory)/'b'
            a = inspect(source, options={'minGenes':1,'maxMitoPct':None}, artifact_dir=a_dir)
            b = inspect(source, options={'minGenes':2,'maxMitoPct':20}, artifact_dir=b_dir)
            result = compare_selections(a_dir,a['selection'],b_dir,b['selection'])
            self.assertTrue(result['comparable'])
            self.assertEqual([result[k] for k in ['bothRetained','removed','added','bothExcluded']], [2,1,0,1])
            reverse = compare_selections(b_dir,b['selection'],a_dir,a['selection'])
            self.assertEqual(reverse['added'],1)
            changed = Path(directory)/'different.h5ad'
            data = ad.read_h5ad(source); data.uns['different'] = True; data.write_h5ad(changed)
            c_dir=Path(directory)/'c'; c=inspect(changed,options={'minGenes':1},artifact_dir=c_dir)
            self.assertFalse(compare_selections(a_dir,a['selection'],c_dir,c['selection'])['comparable'])
            with (b_dir/'cells.npz').open('ab') as stream: stream.write(b'tamper')
            with self.assertRaisesRegex(ValueError,'selection_changed'):
                compare_selections(a_dir,a['selection'],b_dir,b['selection'])

    def test_all_cell_results_and_retained_h5ad_preserve_alignment_and_provenance(self):
        with tempfile.TemporaryDirectory() as directory:
            source = self.make_input(directory); before = file_digest(source)
            artifact = Path(directory) / 'selection'
            report = inspect(source, options={'donorColumn':'subject','minGenes':2,'maxMitoPct':20}, artifact_dir=artifact)
            selection = report['selection']
            manifest, arrays = load_selection(artifact, selection)
            np.testing.assert_array_equal(arrays['retained'], [True,False,False,True])
            self.assertEqual(report['qc']['retained'], 2)
            self.assertEqual(report['qc']['excluded'], 2)
            self.assertEqual(arrays['reason_bits'][1] & 1, 1)
            bundle = Path(directory) / 'qc.zip'
            export_selection(source, artifact, bundle, selection, 'export_qc')
            with zipfile.ZipFile(bundle) as archive:
                saved = json.loads(archive.read('manifest.json'))
                rows = list(csv.DictReader(io.StringIO(archive.read('cells.tsv').decode()), delimiter='\t'))
            self.assertEqual(len(rows), 4)
            self.assertEqual([row['retained'] for row in rows], ['True','False','False','True'])
            self.assertEqual(rows[1]['cell_id'], 'duplicate')
            self.assertEqual(rows[1]['donor_missing'], 'True')
            self.assertEqual(rows[1]['mito_percent'], '')
            self.assertEqual(saved['qc']['options']['donorColumn'], 'subject')
            self.assertEqual(saved['source']['sha256'], before)
            destination = Path(directory) / 'retained.h5ad'
            receipt = export_selection(source, artifact, destination, selection, 'export_h5ad')
            output = ad.read_h5ad(destination); original = ad.read_h5ad(source)
            self.assertEqual(output.shape, (2,3))
            self.assertEqual(list(output.obs_names), ['duplicate','c3'])
            np.testing.assert_array_equal(output.X.toarray(), original.X[[0,3],:].toarray())
            np.testing.assert_array_equal(output.layers['counts'].toarray(), original.layers['counts'][[0,3],:].toarray())
            np.testing.assert_array_equal(output.raw.X.toarray(), original.raw.X[[0,3],:].toarray())
            np.testing.assert_array_equal(output.obsm['embedding'], original.obsm['embedding'][[0,3]])
            np.testing.assert_array_equal(output.obsp['pairwise'].toarray(), np.eye(2))
            self.assertEqual(output.uns['original'], 'preserve me')
            provenance = output.uns[receipt['provenanceKey']]
            self.assertEqual(json.loads(provenance['manifest_json']), manifest)
            columns = json.loads(provenance['obs_columns_json'])
            self.assertEqual(list(output.obs[columns['source_row']]), [0,3])
            self.assertEqual(list(output.obs[columns['donor_id']]), ['A','B'])
            self.assertEqual(file_digest(source), before)
            self.assertEqual(receipt['sha256'], file_digest(destination))

    def test_changed_source_or_artifacts_and_original_alias_cannot_be_exported(self):
        with tempfile.TemporaryDirectory() as directory:
            source = self.make_input(directory); artifact = Path(directory)/'selection'
            report = inspect(source, options={'minGenes':0}, artifact_dir=artifact)
            output = Path(directory)/'out.h5ad'
            with self.assertRaisesRegex(ValueError,'source_conflict'):
                export_selection(source,artifact,source,report['selection'],'export_h5ad')
            alias = Path(directory)/'alias.h5ad'; os.link(source,alias)
            with self.assertRaisesRegex(ValueError,'source_conflict'):
                export_selection(source,artifact,alias,report['selection'],'export_h5ad')
            with source.open('ab') as stream: stream.write(b'changed')
            with self.assertRaisesRegex(ValueError,'file_changed'):
                export_selection(source,artifact,output,report['selection'],'export_h5ad')
            self.assertFalse(output.exists())
            # The full QC archive remains usable without rereading a moved/changed source.
            export_selection(source,artifact,Path(directory)/'qc.zip',report['selection'],'export_qc')
            with (artifact/'cells.npz').open('ab') as stream: stream.write(b'tampered')
            with self.assertRaisesRegex(ValueError,'selection_changed'):
                export_selection(source,artifact,output,report['selection'],'export_h5ad')

    def test_empty_selection_can_be_saved_but_not_written_as_retained_h5ad(self):
        with tempfile.TemporaryDirectory() as directory:
            source = self.make_input(directory); artifact = Path(directory)/'selection'
            report = inspect(source, options={'minGenes':100}, artifact_dir=artifact)
            self.assertEqual(report['selection']['retained'],0)
            export_selection(source,artifact,Path(directory)/'empty-qc.zip',report['selection'],'export_qc')
            with self.assertRaisesRegex(ValueError,'empty_selection'):
                export_selection(source,artifact,Path(directory)/'empty.h5ad',report['selection'],'export_h5ad')

    def test_invalid_counts_do_not_create_a_selection(self):
        with tempfile.TemporaryDirectory() as directory:
            source = self.make_input(directory)
            data = ad.read_h5ad(source); data.X.data[0] = -1; data.write_h5ad(source)
            report = inspect(source, artifact_dir=Path(directory)/'selection')
            self.assertNotIn('selection',report)
            self.assertFalse((Path(directory)/'selection/cells.npz').exists())

    @unittest.skipUnless(os.environ.get('CAUSALMED_TEST_DONOR_H5AD'), 'Set public Kang2018 path')
    def test_public_donor_file_round_trip(self):
        source = Path(os.environ['CAUSALMED_TEST_DONOR_H5AD']); before = file_digest(source)
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory)/'selection'
            report = inspect(source, options={'donorColumn':'replicate'}, artifact_dir=artifact)
            output = Path(directory)/'kang-retained.h5ad'
            receipt = export_selection(source, artifact, output, report['selection'], 'export_h5ad')
            data = ad.read_h5ad(output,backed='r')
            try:
                self.assertEqual(data.shape, (24562,15706))
                self.assertEqual(data.obs['replicate'].nunique(),8)
                self.assertEqual(json.loads(data.uns[receipt['provenanceKey']]['manifest_json'])['qc']['options']['donorColumn'],'replicate')
            finally: data.file.close()
        self.assertEqual(file_digest(source),before)

"""Synthetic HDF5 fixtures verify software contracts, never biomedical findings."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import anndata as ad
import numpy as np
import pandas as pd
from scipy import sparse
from causalmed_worker.h5ad import inspect
from causalmed_worker.selection import export_selection

class CohortExportTests(unittest.TestCase):
    def test_exact_intersection_lineage_donor_flags_and_streamed_matrix(self):
        for matrix in (np.asarray, sparse.csr_matrix, sparse.csc_matrix):
            with self.subTest(matrix=matrix.__name__), tempfile.TemporaryDirectory() as directory:
                folder=Path(directory);source=folder/'synthetic.h5ad';run=folder/'run'
                data=ad.AnnData(matrix([[1,2],[3,4],[5,6],[0,0],[7,8]]),obs=pd.DataFrame({'donor':['D1','D2','D1','D2',None],'condition':pd.Categorical(['A','A','B','A','A']),'perturbation':pd.Categorical(['ctrl','stim','ctrl','ctrl','ctrl'])},index=['x','y','z','zero','missing']))
                data.layers['counts']=data.X.copy();data.write_h5ad(source)
                report=inspect(source,options={'donorColumn':'donor','minGenes':0},artifact_dir=run)
                raw=json.dumps({'id':'00000000-0000-4000-8000-000000000001','sourceSha256':report['sha256'],'metadata':{'donorColumn':'donor','conditionColumn':'condition','perturbationColumn':'perturbation'},'conditionValue':'A','perturbationValue':'ctrl','alignment':{'observation':{'qcId':report['selection']['id'],'manifestSha256':report['selection']['manifestSha256']},'settings':{'mappings':[{'label':'D1','canonicalId':'QA-common-1','reviewed':False},{'label':'D2','canonicalId':'QA-common-2','reviewed':True}]}}},ensure_ascii=False)
                definition={'json':raw,'sha256':hashlib.sha256(raw.encode()).hexdigest()}
                output=folder/'cohort.h5ad';receipt=export_selection(source,run,output,report['selection'],'export_cohort',definition=definition)
                self.assertEqual((receipt['retained'],receipt['matched']),(2,3))
                result=ad.read_h5ad(output);self.assertEqual(list(result.obs_names),['x','missing'])
                actual=result.X.toarray() if sparse.issparse(result.X) else result.X
                original=data.X.toarray() if sparse.issparse(data.X) else data.X
                np.testing.assert_array_equal(actual,original[[0,4]])
                prefix=receipt['provenanceKey']
                self.assertEqual(list(result.obs[prefix+'_canonical_donor_id']),['QA-common-1',''])
                self.assertEqual(list(result.obs[prefix+'_canonical_donor_reviewed']),[False,False])
                self.assertEqual(list(result.obs[prefix+'_source_row']),[0,4])
                origin=result.uns['causalmed_cohort_00000000000040008000000000000001']
                self.assertEqual(origin['definition_json'],raw)
                child=inspect(output,options={'donorColumn':'donor','minGenes':0})
                self.assertEqual(child['cohortOrigins'][0]['definitionSha256'],definition['sha256'])
                self.assertEqual(child['qc']['annotationContext']['condition']['counts'],[{'label':'A','count':2}])
                self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(),report['sha256'])
                broken={**definition,'sha256':'a'*64}
                with self.assertRaisesRegex(ValueError,'invalid_export'):
                    export_selection(source,run,folder/'bad.h5ad',report['selection'],'export_cohort',definition=broken)
                empty=json.loads(raw);empty['conditionValue']='absent';empty=json.dumps(empty);empty={'json':empty,'sha256':hashlib.sha256(empty.encode()).hexdigest()}
                with self.assertRaisesRegex(ValueError,'empty_selection'):
                    export_selection(source,run,folder/'empty.h5ad',report['selection'],'export_cohort',definition=empty)

    def test_multi_value_or_and_qc_missing_and_invalid_sets(self):
        for matrix in (np.asarray, sparse.csr_matrix, sparse.csc_matrix):
            with self.subTest(matrix=matrix.__name__), tempfile.TemporaryDirectory() as directory:
                folder=Path(directory); source=folder/'synthetic.h5ad'; run=folder/'run'
                data=ad.AnnData(matrix([[1,2],[3,4],[5,6],[0,0],[7,8],[9,10],[11,12]]),obs=pd.DataFrame({'donor':['D1']*7,'condition':['A','B','C','A',None,'B','A'],'perturbation':['ctrl','stim','ctrl','ctrl','ctrl','other',None]},index=[f'cell{i}' for i in range(7)]))
                data.layers['counts']=data.X.copy();data.write_h5ad(source)
                report=inspect(source,options={'donorColumn':'donor','minGenes':0},artifact_dir=run)
                payload={'id':'00000000-0000-4000-8000-000000000001','sourceSha256':report['sha256'],'metadata':{'donorColumn':'donor','conditionColumn':'condition','perturbationColumn':'perturbation'},'conditionValue':'A','perturbationValue':'ctrl','conditionValues':['A','B'],'perturbationValues':['ctrl','stim'],'alignment':{'observation':{'qcId':report['selection']['id'],'manifestSha256':report['selection']['manifestSha256']},'settings':{'mappings':[{'label':'D1','canonicalId':'QA-only','reviewed':False}]}}}
                def definition(p):
                    raw=json.dumps(p);return {'json':raw,'sha256':hashlib.sha256(raw.encode()).hexdigest()}
                output=folder/'multi.h5ad'
                receipt=export_selection(source,run,output,report['selection'],'export_cohort',definition=definition(payload))
                self.assertEqual((receipt['retained'],receipt['matched']),(2,3))
                result=ad.read_h5ad(output)
                self.assertEqual(list(result.obs_names),['cell0','cell1'])
                actual=result.layers['counts'].toarray() if sparse.issparse(result.X) else result.layers['counts']
                np.testing.assert_array_equal(actual,[[1,2],[3,4]])
                for invalid in (['A'],['B','A'],['A','A'],['A',''],['B','C']):
                    with self.subTest(invalid=invalid), self.assertRaisesRegex(ValueError,'invalid_export'):
                        export_selection(source,run,folder/'bad.h5ad',report['selection'],'export_cohort',definition=definition({**payload,'conditionValues':invalid}))
                    self.assertFalse((folder/'bad.h5ad').exists())
                self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(),report['sha256'])

"""Synthetic wide-annotation export memory probe; run with PYTHONPATH=python.

Measures export high-water growth after imports/fixture/QC vectors are prepared.
HDF5 fill values keep fixture construction from allocating the numeric payload.
This is an allocation regression probe, not an OS memory cap or biological evidence.
"""
import json
from pathlib import Path
import resource
import sys
import tempfile


def main():
    import anndata as ad
    import h5py
    import numpy as np
    import pandas as pd
    from causalmed_worker.stream_export import stream_retained_h5ad
    rows, columns = 65536, 256
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        data = ad.AnnData(np.ones((rows,2),dtype='float32'),
                          obs=pd.DataFrame(index=[f'synthetic-{i}' for i in range(rows)]))
        data.write_h5ad(root/'source.h5ad')
        with h5py.File(root/'source.h5ad','r+') as handle:
            names = [f'annotation_{i}' for i in range(columns)]
            handle['obs'].attrs['column-order'] = names
            for name in names:
                node = handle['obs'].create_dataset(name,(rows,),dtype='float64',fillvalue=7)
                node.attrs['encoding-type'] = 'array'
                node.attrs['encoding-version'] = '0.2.0'
        arrays = {'cell_id':np.asarray(data.obs_names,dtype=str),'source_row':np.arange(rows),
                  'retained':np.ones(rows,dtype=bool),'total_counts':np.full(rows,2.),
                  'detected_genes':np.full(rows,2),'mito_percent':np.full(rows,np.nan),
                  'donor_id':np.full(rows,'synthetic'),'donor_missing':np.zeros(rows,dtype=bool)}
        manifest = {'analysisId':'00000000-0000-4000-8000-000000000002',
                    'source':{'cells':rows,'genes':2},'purpose':'Synthetic allocation probe only'}
        scale = 1 if sys.platform == 'darwin' else 1024
        before = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale
        _, stats = stream_retained_h5ad(root/'source.h5ad',root/'out.h5ad',manifest,arrays,
                                       {'manifestSha256':'synthetic'},lambda *a:None,64*1024)
        growth = max(0,resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*scale-before)
        with h5py.File(root/'out.h5ad') as output:
            assert output['obs/annotation_255'].shape == (rows,)
            assert float(output['obs/annotation_255'][rows-1]) == 7
            assert len(output['obs'].attrs['column-order']) == columns+7
        print(json.dumps(dict(synthetic=True,retainedAnnotationBytes=rows*columns*8,
                              rssGrowthBytes=growth,**stats)))
        assert growth < 64*1024*1024, growth
        assert stats['maxReadBlockBytes'] <= stats['blockBudgetBytes'], stats
        assert stats['maxGeneratedBlockBytes'] <= stats['blockBudgetBytes'], stats
        assert stats['generatedBlocks'] > 7, stats


if __name__ == '__main__':
    main()

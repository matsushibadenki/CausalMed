"""Synthetic virtual dense matrix memory probe; run with PYTHONPATH=python.

The HDF5 fill-value source avoids allocating the full input during setup.
This measures process high-water growth in a fresh subprocess, not a hard RSS cap.
"""
import json
from pathlib import Path
import resource
import sys
import tempfile
import anndata as ad
import h5py
import numpy as np
import pandas as pd
from causalmed_worker.stream_export import stream_retained_h5ad


def rss():
    value = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return value if sys.platform == 'darwin' else value * 1024


def main():
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory); n, p = 24000, 4096
        data = ad.AnnData(obs=pd.DataFrame(index=[f'synthetic-{i}' for i in range(n)]), var=pd.DataFrame(index=[f'g{i}' for i in range(p)]))
        data.write_h5ad(root/'source.h5ad')
        with h5py.File(root/'source.h5ad','r+') as f:
            if 'X' in f: del f['X']
            x=f.create_dataset('X',(n,p),dtype='float32',fillvalue=1)
            x.attrs['encoding-type']='array';x.attrs['encoding-version']='0.2.0'
        keep=np.arange(n)%2==0
        arrays={'cell_id':np.asarray(data.obs_names,dtype=str),'source_row':np.arange(n),'retained':keep,
                'total_counts':np.full(n,p),'detected_genes':np.full(n,p),'mito_percent':np.zeros(n),
                'donor_id':np.full(n,'synthetic'),'donor_missing':np.zeros(n,dtype=bool)}
        manifest={'analysisId':'00000000-0000-4000-8000-000000000001','source':{'cells':n,'genes':p},'purpose':'synthetic memory probe, not biological data'}
        before=rss()
        _,stats=stream_retained_h5ad(root/'source.h5ad',root/'out.h5ad',manifest,arrays,{'manifestSha256':'synthetic'},lambda *a:None,2*1024*1024)
        growth=max(0,rss()-before)
        with h5py.File(root/'out.h5ad') as f:
            assert f['X'].shape==(12000,p)
            assert f['X'][11999,p-1]==1
        assert stats['maxReadBlockBytes']<=stats['blockBudgetBytes']
        assert growth < 128*1024*1024, f'Unexpected high-water growth: {growth}'
        print(json.dumps({'sourceDenseBytes':n*p*4,'retainedDenseBytes':int(keep.sum())*p*4,'rssGrowthBytes':growth,**stats}))

if __name__=='__main__':main()

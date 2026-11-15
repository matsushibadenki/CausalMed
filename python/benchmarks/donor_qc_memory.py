"""Synthetic donor-QC allocation comparison; no biological evidence.

Run with PYTHONPATH=python in the project environment. A numeric synthetic row index avoids masking donor allocations with temporary
string-index decoding peaks. Index loading occurs before measurement. Fresh subprocesses must produce identical bounded summaries.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    import anndata as ad
    import h5py
    import numpy as np
    import pandas as pd
    count = 1048576
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory)/'condition.synthetic.h5ad'
        from anndata.io import write_elem
        with h5py.File(path,'w') as handle:
            obs = handle.create_group('obs')
            obs.attrs['encoding-type'] = 'dataframe'; obs.attrs['encoding-version'] = '0.2.0'
            obs.attrs['_index'] = '_index'; obs.attrs['column-order'] = ['condition']
            index = obs.create_dataset('_index',(count,),dtype='int64',fillvalue=0)
            index.attrs['encoding-type'] = 'array'; index.attrs['encoding-version'] = '0.2.0'
            write_elem(handle,'var',pd.DataFrame(index=['g']))
            node = handle['obs'].create_dataset('condition',(count,),dtype='int64')
            node.attrs['encoding-type'] = 'array'; node.attrs['encoding-version'] = '0.2.0'
            block = np.arange(8192,dtype=np.int64)
            for start in range(0,count,len(block)):
                node[start:start+len(block)] = block + start
        code = '''
import hashlib,json,resource,sys,numpy as np
from causalmed_worker.backed import open_research_h5ad
from causalmed_worker.qc import donor_summary,donor_selection
frame = open_research_h5ad(sys.argv[1])
passed=np.arange(frame.n_obs)%2==0
scale = 1 if sys.platform == 'darwin' else 1024
before = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*scale
if sys.argv[2] == 'full':
    labels=frame.obs['condition'].astype('string')
    absent=labels.isna() | labels.str.strip().eq('').fillna(True)
    labels=labels.mask(absent)
    counts=labels.value_counts(sort=False).sort_values(ascending=False,kind='stable')
    kept=labels.iloc[np.flatnonzero(passed)].value_counts()
    result={'distinct':len(counts),'missing':int(absent.sum()),'missingRetained':int((absent.to_numpy(dtype=bool)&passed).sum()),'inventoryComplete':False,'otherCells':int(counts.iloc[30:].sum()),'groups':[{'label':str(k),'cells':int(v),'retained':int(kept.get(k,0))} for k,v in counts.iloc[:30].items()]}
    ids=frame.obs['condition'].astype('string').fillna('').to_numpy(dtype=str)
    missing=np.array([not value.strip() for value in ids],dtype=bool)
else:
    result=donor_summary(frame.obs,'condition',passed)
    ids,missing=donor_selection(frame.obs,'condition')
growth = max(0,resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*scale-before)
frame.file.close()
digest=hashlib.sha256(json.dumps(result,sort_keys=True).encode())
digest.update(ids.tobytes());digest.update(missing.tobytes())
print(json.dumps(dict(mode=sys.argv[2],growthBytes=growth,distinct=result['distinct'],sha256=digest.hexdigest())))
'''
        reports = [json.loads(subprocess.check_output([sys.executable,'-B','-c',code,str(path),mode],text=True)) for mode in ('full','chunked')]
        print(json.dumps(dict(synthetic=True,rows=count,blockRows=8192,results=reports)))
        assert reports[0]['sha256'] == reports[1]['sha256']
        assert reports[1]['distinct'] == count
        assert reports[1]['growthBytes'] < 64*1024*1024, reports
        assert reports[0]['growthBytes']-reports[1]['growthBytes'] > 64*1024*1024, reports



if __name__ == '__main__':
    main()

"""Synthetic annotation-summary allocation comparison; no biological evidence.

Run with PYTHONPATH=python in the project environment. A numeric synthetic row index avoids masking summary allocations with temporary
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
import hashlib,json,resource,sys
from causalmed_worker.backed import open_research_h5ad,annotation_schema,annotation_group
frame = open_research_h5ad(sys.argv[1])
scale = 1 if sys.platform == 'darwin' else 1024
before = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*scale
if sys.argv[2] == 'full':
    series=frame.obs['condition'].astype('string')
    counts=series.dropna().value_counts(sort=False).sort_values(ascending=False,kind='stable')
    result={'distinct':len(counts),'missing':int(series.isna().sum()),'complete':False,'other':int(counts.iloc[20:].sum()),'counts':[{'label':str(k),'count':int(v)} for k,v in counts.iloc[:20].items()]}
else:
    schema=annotation_schema(frame.obs,'condition')
    result=annotation_group(frame.obs,'condition')
growth = max(0,resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*scale-before)
frame.file.close()
print(json.dumps(dict(mode=sys.argv[2],growthBytes=growth,distinct=result['distinct'],sha256=hashlib.sha256(json.dumps(result,sort_keys=True).encode()).hexdigest())))
'''
        reports = [json.loads(subprocess.check_output([sys.executable,'-B','-c',code,str(path),mode],text=True)) for mode in ('full','chunked')]
        print(json.dumps(dict(synthetic=True,rows=count,blockRows=8192,results=reports)))
        assert reports[0]['sha256'] == reports[1]['sha256']
        assert reports[1]['distinct'] == count
        assert reports[1]['growthBytes'] < 32*1024*1024, reports
        assert reports[0]['growthBytes']-reports[1]['growthBytes'] > 32*1024*1024, reports



if __name__ == '__main__':
    main()

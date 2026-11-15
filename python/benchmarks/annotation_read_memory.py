"""Synthetic wide-annotation allocation comparison, not biological evidence.

Run with PYTHONPATH=python using the project Python environment. Fixture payloads
use HDF5 fill values, and readers run in independent fresh subprocesses.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    import anndata as ad
    import h5py
    import pandas as pd
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / 'wide.synthetic.h5ad'
        rows, columns = 262144, 64
        ad.AnnData(obs=pd.DataFrame(index=[f'c{i}' for i in range(rows)]),
                   var=pd.DataFrame(index=['g'])).write_h5ad(path)
        with h5py.File(path,'r+') as handle:
            names = [f'column_{i}' for i in range(columns)]
            handle['obs'].attrs['column-order'] = names
            for name in names:
                node = handle['obs'].create_dataset(name,shape=(rows,),dtype='float64',fillvalue=7)
                node.attrs['encoding-type'] = 'array'
                node.attrs['encoding-version'] = '0.2.0'
        code = '''
import anndata, json, resource, sys
from causalmed_worker.backed import open_research_h5ad
scale = 1 if sys.platform == 'darwin' else 1024
before = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale
reader = open_research_h5ad if sys.argv[2] == 'projection' else lambda path: anndata.read_h5ad(path,backed='r')
data = reader(sys.argv[1])
assert len(data.obs.columns) == 64 and len(data.obs) == 262144
assert float(data.obs['column_0'].sum()) == 262144*7
peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale
data.file.close()
print(json.dumps(dict(mode=sys.argv[2],growthBytes=max(0,peak-before))))
'''
        reports = [json.loads(subprocess.check_output([sys.executable,'-c',code,str(path),mode],text=True)) for mode in ('anndata','projection')]
        print(json.dumps(dict(synthetic=True,annotationBytes=rows*columns*8,results=reports)))
        assert reports[1]['growthBytes'] < 64*1024*1024, reports
        assert reports[0]['growthBytes']-reports[1]['growthBytes'] > 64*1024*1024, reports


if __name__ == '__main__':
    main()

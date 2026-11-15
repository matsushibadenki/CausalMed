"""Compare eager backed AnnData and research projection in fresh processes.

Synthetic virtual layer/embedding/uns arrays exercise allocation only. No
biological findings or prediction performance are measured. Run with PYTHONPATH=python.
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
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / 'synthetic.h5ad'
        data = ad.AnnData(np.ones((4096, 4), dtype='float32'))
        data.write_h5ad(path)
        with h5py.File(path, 'r+') as handle:
            for name in ('layers/unused', 'obsm/unused', 'uns/unused'):
                node = handle.create_dataset(name, shape=(4096,4096 if not name.startswith('layers') else 4), dtype='float32', fillvalue=1)
                node.attrs['encoding-type'] = 'array'
                node.attrs['encoding-version'] = '0.2.0'
        code = '''
import anndata, resource, sys, json
from causalmed_worker.backed import open_research_h5ad
scale = 1 if sys.platform == 'darwin' else 1024
before = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale
reader = open_research_h5ad if sys.argv[2] == 'projection' else lambda path: anndata.read_h5ad(path, backed='r')
data = reader(sys.argv[1])
assert (data.n_obs, data.n_vars) == (4096,4)
peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale
data.file.close()
print(json.dumps(dict(mode=sys.argv[2],peakBytes=peak,growthBytes=max(0,peak-before))))
'''
        reports = [json.loads(subprocess.check_output([sys.executable,'-c',code,str(path),mode],text=True)) for mode in ('anndata','projection')]
        print(json.dumps({'synthetic': True, 'results': reports}))
        assert reports[1]['growthBytes'] < 32*1024*1024, reports
        assert reports[0]['growthBytes'] - reports[1]['growthBytes'] > 64*1024*1024, reports


if __name__ == '__main__':
    main()

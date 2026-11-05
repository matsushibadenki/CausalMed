"""H5AD metadata worker. NDJSON stdout is a bounded progress/result protocol."""
from importlib.metadata import version
import hashlib
import json
from pathlib import Path
import sys


def emit(message):
    print(json.dumps(message, ensure_ascii=False, allow_nan=False), flush=True)


def inspect(path, progress=lambda percent, phase: None):
    progress(1, 'runtime')
    import anndata
    import pandas as pd
    path = Path(path).resolve(strict=True)
    if path.suffix.lower() != '.h5ad' or not path.is_file():
        raise ValueError('invalid_h5ad')
    before = path.stat()
    digest = hashlib.sha256()
    last_percent = -1
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
            percent = 5 + int(45 * stream.tell() / max(before.st_size, 1))
            if percent != last_percent:
                progress(percent, 'checksum')
                last_percent = percent
    progress(52, 'opening')
    data = anndata.read_h5ad(path, backed='r')
    try:
        if data.n_obs == 0 or data.n_vars == 0:
            raise ValueError('empty_h5ad')
        columns = [str(c) for c in data.obs.columns]
        groups = {}
        # Prefer research annotations, then actual categorical annotations (e.g. louvain).
        preferred = ('donor', 'donor_id', 'cell_type', 'celltype', 'perturbation', 'condition', 'louvain', 'leiden')
        candidates = list(dict.fromkeys([k for k in preferred if k in data.obs] + [k for k in data.obs if isinstance(data.obs[k].dtype, pd.CategoricalDtype)]))[:12]
        progress(65, 'annotations')
        for index, key in enumerate(candidates):
            series = data.obs[key].astype('string')
            counts = series.dropna().value_counts()
            groups[str(key)[:200]] = {
                'distinct': len(counts), 'missing': int(series.isna().sum()),
                'other': int(counts.iloc[20:].sum()),
                'counts': [{'label': str(k)[:200], 'count': int(v)} for k, v in counts.iloc[:20].items()],
            }
            progress(65 + int(25 * (index + 1) / max(len(candidates), 1)), 'annotations')
        progress(92, 'validating')
        after = path.stat()
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise ValueError('file_changed')
        return {'schemaVersion': 1, 'name': path.name[:500], 'sha256': digest.hexdigest(),
                'bytes': before.st_size, 'cells': data.n_obs, 'genes': data.n_vars,
                'obsColumns': [c[:200] for c in columns[:200]], 'totalObsColumns': len(columns),
                'duplicateCellIds': int(data.obs_names.duplicated().sum()),
                'duplicateGeneIds': int(data.var_names.duplicated().sum()), 'groups': groups,
                'synthetic': None, 'validation': 'metadata_only', 'anndataVersion': version('anndata')}
    finally:
        data.file.close()


def main():
    try:
        report = inspect(sys.argv[1], lambda percent, phase: emit({'type': 'progress', 'percent': percent, 'phase': phase}))
        emit({'type': 'result', 'report': report})
        return 0
    except ModuleNotFoundError:
        emit({'type': 'error', 'code': 'python_dependencies_missing'})
    except (OSError, ValueError, KeyError, TypeError):
        emit({'type': 'error', 'code': 'invalid_h5ad'})
    except Exception:
        emit({'type': 'error', 'code': 'worker_failed'})
    return 1


if __name__ == '__main__':
    sys.exit(main())

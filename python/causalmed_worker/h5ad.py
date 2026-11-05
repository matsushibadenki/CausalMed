"""H5AD metadata worker. NDJSON stdout is a bounded progress/result protocol."""
from importlib.metadata import version
import hashlib
import json
from pathlib import Path
import sys


def emit(message):
    print(json.dumps(message, ensure_ascii=False, allow_nan=False), flush=True)


if 'quality_control' not in globals():
    from causalmed_worker.qc import quality_control, qc_options


if 'save_selection' not in globals():
    from causalmed_worker.selection import save_selection, export_selection, compare_selections


def inspect(path, progress=lambda percent, phase: None, options=None, expected_sha=None, artifact_dir=None):
    options = qc_options(options)
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
    if expected_sha and digest.hexdigest() != expected_sha:
        raise ValueError('file_changed')
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
            progress(65 + int(5 * (index + 1) / max(len(candidates), 1)), 'annotations')
        selection = {}
        qc = quality_control(data, options, progress, selection.update if artifact_dir else None)
        progress(95, 'validating')
        after = path.stat()
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise ValueError('file_changed')
        report = {'schemaVersion': 1, 'name': path.name[:500], 'sha256': digest.hexdigest(),
                'bytes': before.st_size, 'cells': data.n_obs, 'genes': data.n_vars,
                'obsColumns': [c[:200] for c in columns[:200]], 'totalObsColumns': len(columns),
                'duplicateCellIds': int(data.obs_names.duplicated().sum()),
                'duplicateGeneIds': int(data.var_names.duplicated().sum()), 'groups': groups,
                'synthetic': None, 'validation': 'expression_qc', 'anndataVersion': version('anndata'),
                'matrices': ['X'] + (['raw'] if data.raw is not None else []) + ['layer:' + str(k) for k in list(data.layers)[:30]],
                'geneColumns': [str(k) for k in data.var.columns[:100]],
                'rawGeneColumns': [str(k) for k in data.raw.var.columns[:100]] if data.raw is not None else [], 'qc': qc}
        if artifact_dir and selection:
            progress(97, 'selection')
            report['selection'] = save_selection(artifact_dir, report, selection)
        return report
    finally:
        data.file.close()


def main():
    try:
        job = json.loads(sys.argv[4]) if len(sys.argv) > 4 else {}
        progress = lambda percent, phase: emit({'type': 'progress', 'percent': percent, 'phase': phase})
        if job.get('operation') == 'relink':
            path = Path(sys.argv[1])
            before = path.stat()
            digest = hashlib.sha256()
            with path.open('rb') as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b''):
                    digest.update(block)
                    progress(min(99, int(stream.tell() * 99 / max(before.st_size, 1))), 'checksum')
            after = path.stat()
            if digest.hexdigest() != job['sha256'] or (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
                raise ValueError('file_changed')
            emit({'type': 'result', 'report': {'sha256': digest.hexdigest()}})
        elif job.get('operation') == 'compare':
            emit({'type': 'result', 'report': compare_selections(job['leftDir'], job['left'], job['rightDir'], job['right'])})
        elif job.get('operation', '').startswith('export_'):
            receipt = export_selection(sys.argv[1], job['selectionDir'], job['output'], job['selection'], job['operation'], progress)
            emit({'type': 'export_result', 'report': receipt})
        else:
            report = inspect(sys.argv[1], progress, json.loads(sys.argv[2]) if len(sys.argv) > 2 else None, sys.argv[3] if len(sys.argv) > 3 else None, job.get('artifactDir'))
            emit({'type': 'result', 'report': report})
        return 0
    except ModuleNotFoundError:
        emit({'type': 'error', 'code': 'python_dependencies_missing'})
    except ValueError as error:
        code = str(error) if str(error) in ('invalid_qc_options', 'invalid_donor_column', 'invalid_gene_column', 'invalid_matrix', 'file_changed', 'selection_changed', 'source_conflict', 'empty_selection', 'metadata_collision', 'invalid_export', 'export_verification_failed') else 'invalid_h5ad'
        emit({'type': 'error', 'code': code})
    except (OSError, KeyError, TypeError):
        emit({'type': 'error', 'code': 'invalid_h5ad'})
    except Exception:
        emit({'type': 'error', 'code': 'worker_failed'})
    return 1


if __name__ == '__main__':
    sys.exit(main())

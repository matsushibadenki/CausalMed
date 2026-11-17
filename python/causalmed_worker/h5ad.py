"""H5AD metadata worker. NDJSON stdout is a bounded progress/result protocol."""
from importlib.metadata import version
import errno
import hashlib
import json
import os
from pathlib import Path
import sys

if 'open_research_h5ad' not in globals():
    from causalmed_worker.backed import open_research_h5ad, categorical_columns, annotation_schema, annotation_group, duplicate_frame_ids


def emit(message):
    print(json.dumps(message, ensure_ascii=False, allow_nan=False), flush=True)


if 'quality_control' not in globals():
    from causalmed_worker.qc import quality_control, qc_options


if 'save_selection' not in globals():
    from causalmed_worker.selection import save_selection, export_selection, compare_selections


def inspect(path, progress=lambda percent, phase: None, options=None, expected_sha=None, artifact_dir=None):
    options = qc_options(options)
    progress(1, 'runtime')
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
    data = open_research_h5ad(path)
    try:
        if data.n_obs == 0 or data.n_vars == 0:
            raise ValueError('empty_h5ad')
        columns = [str(c) for c in data.obs.columns]
        obs_schema = []
        for name in columns[:256]:
            if len(name.encode("utf-8")) > 500:
                continue
            obs_schema.append(annotation_schema(data.obs, name))
        groups = {}
        # Prefer research annotations, then actual categorical annotations (e.g. louvain).
        preferred = ('donor', 'donor_id', 'cell_type', 'celltype', 'perturbation', 'condition', 'louvain', 'leiden')
        candidates = [key for key in preferred if key in data.obs]
        for key in categorical_columns(data.obs):
            if len(candidates) >= 12:
                break
            if key not in candidates:
                candidates.append(key)
        progress(65, 'annotations')
        for index, key in enumerate(candidates):
            groups[str(key)[:200]] = annotation_group(data.obs, key)
            progress(65 + int(5 * (index + 1) / max(len(candidates), 1)), 'annotations')
        selection = {}
        qc = quality_control(data, options, progress, selection.update if artifact_dir else None,
                             stream_selection=True)
        # Bind bounded annotation context to the immutable QC manifest.
        qc['annotationContext'] = {key: {**group, 'counts': group['counts'] if group['complete'] and group['distinct'] == 1 else []} for key, group in groups.items()}
        progress(95, 'validating')
        report = {'schemaVersion': 1, 'name': path.name[:500], 'sha256': digest.hexdigest(),
                'bytes': before.st_size, 'cells': data.n_obs, 'genes': data.n_vars,
                'obsColumns': [c[:200] for c in columns[:200]], 'totalObsColumns': len(columns),
                'obsSchema': obs_schema, 'obsSchemaTruncated': len(obs_schema) < len(columns),
                'duplicateCellIds': duplicate_frame_ids(data.obs),
                'duplicateGeneIds': duplicate_frame_ids(data.var), 'groups': groups,
                'synthetic': None, 'validation': 'expression_qc', 'anndataVersion': version('anndata'),
                'matrices': ['X'] + (['raw'] if data.raw is not None else []) + ['layer:' + str(k) for k in list(data.layers)[:30]],
                'geneColumns': [str(k) for k in data.var.columns[:100]],
                'rawGeneColumns': [str(k) for k in data.raw.var.columns[:100]] if data.raw is not None else [], 'qc': qc}
        origins = []
        for key in sorted(data.uns):
            if str(key).startswith('causalmed_cohort_') and len(origins) < 8:
                try:
                    origin = data.uns[key]
                    raw = str(origin['definition_json'])
                    if len(raw.encode('utf-8')) > 65536 or hashlib.sha256(raw.encode('utf-8')).hexdigest() != origin['definition_sha256']:
                        continue
                    payload = json.loads(raw)
                    origins.append({'definitionId': payload['id'], 'definitionSha256': str(origin['definition_sha256']), 'sourceSha256': payload['sourceSha256'], 'parentQcId': payload['alignment']['observation']['qcId'], 'retained': int(origin['retained'])})
                except (KeyError, TypeError, ValueError):
                    continue
        report['cohortOrigins'] = origins
        if artifact_dir and selection:
            progress(97, 'selection')
            report['selection'] = save_selection(artifact_dir, report, selection)
        after = path.stat()
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise ValueError('file_changed')
        return report
    finally:
        data.file.close()


if 'inspect_outcome' not in globals():
    from causalmed_worker.outcome import inspect_outcome


def install_worker_memory_limit():
    """Cap mappings before importing the native scientific dependencies."""
    configured = os.environ.get('CAUSALMED_WORKER_MEMORY_BYTES')
    if configured is None:
        return None
    import resource

    ceiling = int(configured)
    address_space = ceiling
    if sys.platform == 'darwin':
        import ctypes

        # TASK_VM_INFO starts with virtual_size. Darwin charges large pre-existing
        # shared-cache and allocator reservations to this value.
        kernel = ctypes.CDLL(None)
        task = ctypes.c_uint.in_dll(kernel, 'mach_task_self_')
        info = (ctypes.c_uint64 * 128)()
        count = ctypes.c_uint(ctypes.sizeof(info) // ctypes.sizeof(ctypes.c_uint))
        task_vm_info = 22
        if kernel.task_info(task, task_vm_info, ctypes.byref(info), ctypes.byref(count)) or count.value < 2:
            raise ValueError('resource_setup_failed')
        address_space += info[0]
    try:
        for inherited in resource.getrlimit(resource.RLIMIT_AS):
            if inherited != resource.RLIM_INFINITY:
                address_space = min(address_space, inherited)
        resource.setrlimit(resource.RLIMIT_AS, (address_space, address_space))
    except (OSError, ValueError) as error:
        raise ValueError('resource_setup_failed') from error
    return ceiling


def assert_worker_peak(ceiling):
    if ceiling is not None:
        import resource

        peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        if peak * (1 if sys.platform == 'darwin' else 1024) > ceiling:
            raise MemoryError


def is_worker_allocation_failure(error):
    if isinstance(error, MemoryError) or getattr(error, 'errno', None) == errno.ENOMEM:
        return True
    # h5py translates native HDF5 failures into Python exceptions with stack text.
    message = str(error).lower()
    # Linux's dynamic loader can hide ENOMEM behind the extension import error.
    if sys.platform == 'linux' and isinstance(error, ImportError):
        if 'failed to map segment from shared object' in message:
            return True
    return any(fragment in message for fragment in (
        'memory allocation failed', "can't allocate memory", 'unable to allocate memory',
        'cannot allocate memory', 'failed to allocate memory', 'out of memory',
    ))


def main():
    ceiling = None
    try:
        ceiling = install_worker_memory_limit()

        def result(kind, report):
            assert_worker_peak(ceiling)
            emit({'type': kind, 'report': report})

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
            result('result', {'sha256': digest.hexdigest()})
        elif job.get('operation') == 'inspect_outcome':
            result('result', inspect_outcome(job['context'], job['contextSha256'], job['inputs'], progress))
        elif job.get('operation') == 'compare':
            result('result', compare_selections(job['leftDir'], job['left'], job['rightDir'], job['right']))
        elif job.get('operation', '').startswith('export_'):
            receipt = export_selection(sys.argv[1], job['selectionDir'], job['output'], job['selection'], job['operation'], progress, job.get('definition'))
            result('export_result', receipt)
        else:
            report = inspect(sys.argv[1], progress, json.loads(sys.argv[2]) if len(sys.argv) > 2 else None, sys.argv[3] if len(sys.argv) > 3 else None, job.get('artifactDir'))
            result('result', report)
        return 0
    except Exception as error:
        try:
            assert_worker_peak(ceiling)
            memory_failure = ceiling is not None and is_worker_allocation_failure(error)
        except MemoryError:
            memory_failure = True
        if memory_failure:
            code = 'worker_memory_limit'
        elif isinstance(error, ModuleNotFoundError):
            code = 'python_dependencies_missing'
        elif isinstance(error, ValueError):
            code = str(error) if str(error) in (
                'invalid_qc_options', 'invalid_donor_column', 'invalid_gene_column', 'invalid_matrix',
                'file_changed', 'selection_changed', 'source_conflict', 'empty_selection',
                'metadata_collision', 'invalid_export', 'export_verification_failed',
                'unsupported_export_encoding', 'vlen_budget_exceeded', 'resource_setup_failed',
            ) else 'invalid_h5ad'
        elif isinstance(error, (OSError, KeyError, TypeError)):
            code = 'invalid_h5ad'
        else:
            code = 'worker_failed'
        emit({'type': 'error', 'code': code})
    return 1


if __name__ == '__main__':
    sys.exit(main())

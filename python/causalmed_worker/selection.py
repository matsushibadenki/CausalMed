"""Immutable, per-cell QC selections and read-only-source export helpers."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import uuid

QC_REASONS = ['zeroCounts', 'lowGenes', 'highGenes', 'lowCounts', 'highCounts', 'highMito']


def file_digest(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def save_selection(directory, report, arrays):
    import numpy as np
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    selection_path = directory / 'cells.npz'
    with selection_path.open('xb') as stream:
        np.savez_compressed(stream, **arrays)
    analysis_id = str(uuid.uuid4())
    created = datetime.now(timezone.utc).isoformat()
    manifest = {
        'schemaVersion': 1, 'analysisId': analysis_id, 'createdAt': created,
        'algorithm': 'causalmed-count-qc-v1', 'anndataVersion': report['anndataVersion'],
        'source': {key: report[key] for key in ('name', 'sha256', 'cells', 'genes')},
        'cellIdentity': 'zero-based source row index plus original obs_name; duplicates preserved',
        'reasonBits': {name: 1 << index for index, name in enumerate(QC_REASONS)},
        'qc': report['qc'], 'selectionSha256': file_digest(selection_path),
    }
    manifest_path = directory / 'manifest.json'
    with manifest_path.open('x', encoding='utf-8') as stream:
        json.dump(manifest, stream, ensure_ascii=False, allow_nan=False, sort_keys=True)
    return {'id': analysis_id, 'createdAt': created, 'sha256': manifest['selectionSha256'],
            'manifestSha256': file_digest(manifest_path), 'retained': report['qc']['retained'],
            'excluded': report['qc']['excluded']}


def load_selection(directory, expected):
    import numpy as np
    directory = Path(directory)
    manifest_path, cells_path = directory / 'manifest.json', directory / 'cells.npz'
    if file_digest(manifest_path) != expected['manifestSha256'] or file_digest(cells_path) != expected['sha256']:
        raise ValueError('selection_changed')
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    if manifest['analysisId'] != expected['id'] or manifest['selectionSha256'] != expected['sha256']:
        raise ValueError('selection_changed')
    with np.load(cells_path, allow_pickle=False) as archive:
        arrays = {key: archive[key] for key in archive.files}
    n = manifest['source']['cells']
    required = ['source_row', 'cell_id', 'retained', 'reason_bits', 'total_counts', 'detected_genes', 'mito_percent', 'donor_id', 'donor_missing']
    if any(key not in arrays or arrays[key].shape != (n,) for key in required):
        raise ValueError('selection_changed')
    if not np.array_equal(arrays['source_row'], np.arange(n)) or arrays['retained'].dtype != np.bool_:
        raise ValueError('selection_changed')
    if not np.array_equal(arrays['retained'], arrays['reason_bits'] == 0) or int(arrays['retained'].sum()) != manifest['qc']['retained']:
        raise ValueError('selection_changed')
    return manifest, arrays


def export_selection(source, directory, output, expected, kind, progress=lambda percent, phase: None):
    import numpy as np
    progress(2, 'selection')
    manifest, arrays = load_selection(directory, expected)
    output = Path(output)
    if output.resolve() == Path(source).resolve() or (output.exists() and Path(source).exists() and output.samefile(source)):
        raise ValueError('source_conflict')
    # The caller owns a private staging file; final publication is performed by Rust.
    if kind == 'export_qc':
        import csv
        import io
        import zipfile
        with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr('manifest.json', json.dumps(manifest, ensure_ascii=False, allow_nan=False, sort_keys=True))
            with archive.open('cells.tsv', 'w') as raw:
                with io.TextIOWrapper(raw, encoding='utf-8', newline='') as stream:
                    writer = csv.writer(stream, delimiter='\t')
                    writer.writerow(['source_row', 'cell_id', 'retained', 'reason_bits', *QC_REASONS, 'total_counts', 'detected_genes', 'mito_percent', 'donor_id', 'donor_missing'])
                    n = len(arrays['retained'])
                    for index in range(n):
                        bits = int(arrays['reason_bits'][index])
                        writer.writerow([index, arrays['cell_id'][index], bool(arrays['retained'][index]), bits,
                                         *[bool(bits & (1 << bit)) for bit in range(len(QC_REASONS))],
                                         float(arrays['total_counts'][index]), int(arrays['detected_genes'][index]),
                                         float(arrays['mito_percent'][index]) if np.isfinite(arrays['mito_percent'][index]) else '',
                                         arrays['donor_id'][index], bool(arrays['donor_missing'][index])])
                        if index % max(n // 20, 1) == 0:
                            progress(10 + int(index / n * 75), 'writing')
        provenance_key = ''
    elif kind == 'export_h5ad':
        import anndata
        progress(5, 'checksum')
        if not Path(source).is_file() or file_digest(source) != manifest['source']['sha256']:
            raise ValueError('file_changed')
        indices = np.flatnonzero(arrays['retained'])
        if not len(indices):
            raise ValueError('empty_selection')
        before = Path(source).stat()
        data = anndata.read_h5ad(source, backed='r')
        try:
            if data.shape != (manifest['source']['cells'], manifest['source']['genes']) or not np.array_equal(np.asarray(data.obs_names, dtype=str), arrays['cell_id']):
                raise ValueError('file_changed')
            progress(30, 'writing')
            subset = data[indices, :].to_memory()
            provenance_key = 'causalmed_qc_' + manifest['analysisId'].replace('-', '')
            columns = {key: provenance_key + '_' + key for key in ('source_row', 'retained', 'total_counts', 'detected_genes', 'mito_percent', 'donor_id', 'donor_missing')}
            if provenance_key in subset.uns or any(key in subset.obs for key in columns.values()):
                raise ValueError('metadata_collision')
            for key, column in columns.items():
                subset.obs[column] = arrays[key][indices]
            subset.uns[provenance_key] = {
                'manifest_json': json.dumps(manifest, ensure_ascii=False, allow_nan=False, sort_keys=True),
                'obs_columns_json': json.dumps(columns, sort_keys=True),
                'selection_manifest_sha256': expected['manifestSha256'],
            }
            subset.write_h5ad(output, compression='gzip')
            del subset
            progress(80, 'export_verify')
            check = anndata.read_h5ad(output, backed='r')
            try:
                if check.shape != (len(indices), data.n_vars) or not np.array_equal(np.asarray(check.obs_names, dtype=str), arrays['cell_id'][indices]):
                    raise ValueError('export_verification_failed')
                if check.uns[provenance_key]['manifest_json'] != json.dumps(manifest, ensure_ascii=False, allow_nan=False, sort_keys=True):
                    raise ValueError('export_verification_failed')
            finally:
                check.file.close()
        finally:
            data.file.close()
        after = Path(source).stat()
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns) or file_digest(source) != manifest['source']['sha256']:
            raise ValueError('file_changed')
    else:
        raise ValueError('invalid_export')
    progress(95, 'export_verify')
    return {'kind': kind, 'selectionId': manifest['analysisId'], 'sourceSha256': manifest['source']['sha256'],
            'sha256': file_digest(output), 'bytes': output.stat().st_size,
            'retained': manifest['qc']['retained'], 'excluded': manifest['qc']['excluded'],
            'provenanceKey': provenance_key, 'createdAt': datetime.now(timezone.utc).isoformat()}


def compare_selections(left_dir, left, right_dir, right):
    import numpy as np
    a, aa = load_selection(left_dir, left)
    b, bb = load_selection(right_dir, right)
    result = {'leftId': left['id'], 'rightId': right['id'], 'comparable': False}
    if a['source']['sha256'] != b['source']['sha256']:
        return result
    if not np.array_equal(aa['source_row'], bb['source_row']) or not np.array_equal(aa['cell_id'], bb['cell_id']):
        raise ValueError('selection_changed')
    ar, br = aa['retained'], bb['retained']
    result.update(comparable=True, bothRetained=int((ar & br).sum()), bothExcluded=int((~ar & ~br).sum()),
                  removed=int((ar & ~br).sum()), added=int((~ar & br).sum()))
    return result

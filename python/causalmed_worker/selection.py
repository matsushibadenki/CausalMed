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


def export_selection(source, directory, output, expected, kind, progress=lambda percent, phase: None, definition=None):
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
    elif kind in ('export_h5ad', 'export_cohort'):
        progress(5, 'checksum')
        if not Path(source).is_file() or file_digest(source) != manifest['source']['sha256']:
            raise ValueError('file_changed')
        if not np.any(arrays['retained']):
            raise ValueError('empty_selection')
        before = Path(source).stat()
        matched = manifest['source']['cells']
        if kind == 'export_cohort':
            import anndata as ad
            if not isinstance(definition, dict) or len(definition.get('json', '').encode('utf-8')) > 65536 or hashlib.sha256(definition.get('json', '').encode('utf-8')).hexdigest() != definition.get('sha256'):
                raise ValueError('invalid_export')
            payload = json.loads(definition['json'])
            observation = payload['alignment']['observation']
            if payload['sourceSha256'] != manifest['source']['sha256'] or observation['qcId'] != expected['id'] or observation['manifestSha256'] != expected['manifestSha256'] or payload['metadata']['donorColumn'] != manifest['qc']['options']['donorColumn']:
                raise ValueError('selection_changed')
            data = ad.read_h5ad(source, backed='r')
            try:
                selected = np.ones(data.n_obs, dtype=bool)
                for role, value in (('conditionColumn', payload['conditionValue']), ('perturbationColumn', payload['perturbationValue'])):
                    column = payload['metadata'][role]
                    if not column or column not in data.obs or not isinstance(value, str) or not value.strip():
                        raise ValueError('invalid_export')
                    selected &= data.obs[column].astype('string').eq(value).fillna(False).to_numpy(dtype=bool)
            finally:
                data.file.close()
            matched = int(selected.sum())
            arrays['retained'] = arrays['retained'] & selected
            if not arrays['retained'].any():
                raise ValueError('empty_selection')
            mappings = {m['label']: m for m in payload['alignment']['settings']['mappings']}
            arrays['canonical_donor_id'] = np.asarray([mappings.get(str(label), {}).get('canonicalId', '') for label in arrays['donor_id']], dtype=str)
            arrays['canonical_donor_reviewed'] = np.asarray([mappings.get(str(label), {}).get('reviewed', False) for label in arrays['donor_id']], dtype=bool)
            manifest['cohort'] = {'definition_json': definition['json'], 'definition_sha256': definition['sha256'], 'matched': matched, 'retained': int(arrays['retained'].sum()), 'source_rows_sha256': hashlib.sha256(arrays['source_row'][arrays['retained']].astype('<i8').tobytes()).hexdigest(), 'selection_rule': 'exact condition AND perturbation AND parent QC retained; missing values excluded'}
        writer = globals().get('stream_retained_h5ad')
        if writer is None:
            from causalmed_worker.stream_export import stream_retained_h5ad as writer
        provenance_key, streaming = writer(source, output, manifest, arrays, expected, progress)
        after = Path(source).stat()
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns) or file_digest(source) != manifest['source']['sha256']:
            raise ValueError('file_changed')
    else:
        raise ValueError('invalid_export')
    progress(95, 'export_verify')
    return {'kind': kind, 'selectionId': manifest['analysisId'], 'sourceSha256': manifest['source']['sha256'],
            'sha256': file_digest(output), 'bytes': output.stat().st_size,
            'retained': int(arrays['retained'].sum()), 'excluded': manifest['source']['cells'] - int(arrays['retained'].sum()),
            'definitionId': payload['id'] if kind == 'export_cohort' else None, 'definitionSha256': definition['sha256'] if kind == 'export_cohort' else None, 'matched': matched if kind == 'export_cohort' else None,
            'provenanceKey': provenance_key, 'streaming': streaming if kind in ('export_h5ad', 'export_cohort') else None, 'createdAt': datetime.now(timezone.utc).isoformat()}


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

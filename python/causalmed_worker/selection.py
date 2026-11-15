"""Immutable, per-cell QC selections and read-only-source export helpers."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import uuid

if 'open_research_h5ad' not in globals():
    from causalmed_worker.backed import open_research_h5ad, iter_annotation_column, scratch_vector

if 'DonorExportColumn' not in globals():
    from causalmed_worker.stream_export import DonorExportColumn

QC_REASONS = ['zeroCounts', 'lowGenes', 'highGenes', 'lowCounts', 'highCounts', 'highMito']


def file_digest(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def save_selection(directory, report, arrays):
    import numpy as np
    import zipfile
    if 'QcArray' not in globals():
        from causalmed_worker.qc import QcArray
    else:
        QcArray = globals()['QcArray']

    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    selection_path = directory / 'cells.npz'
    with zipfile.ZipFile(selection_path, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for key, values in arrays.items():
            with archive.open(key + '.npy', 'w', force_zip64=True) as member:
                if isinstance(values, QcArray):
                    values.write(member)
                else:
                    np.lib.format.write_array(member, values, allow_pickle=False)
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
    n = manifest['source']['cells']
    arrays = map_selection_arrays(cells_path, n)
    if arrays['retained'].dtype != np.bool_:
        raise ValueError('selection_changed')
    retained = 0
    for start in range(0, n, 8192):
        stop = min(start + 8192, n)
        block = slice(start, stop)
        if (not np.array_equal(arrays['source_row'][block], np.arange(start, stop))
                or not np.array_equal(arrays['retained'][block], arrays['reason_bits'][block] == 0)):
            raise ValueError('selection_changed')
        retained += int(arrays['retained'][block].sum())
    if retained != manifest['qc']['retained']:
        raise ValueError('selection_changed')
    return manifest, arrays


def map_selection_arrays(path, length):
    """Inflate each NPY member to anonymous disk storage, then map it read-only."""
    import shutil
    import tempfile
    import zipfile
    import numpy as np

    required = {
        'source_row': 'iu', 'cell_id': 'U', 'retained': 'b', 'reason_bits': 'iu',
        'total_counts': 'fiu', 'detected_genes': 'iu', 'mito_percent': 'f',
        'donor_id': 'U', 'donor_missing': 'b',
    }
    arrays = {}
    try:
        with zipfile.ZipFile(path) as archive:
            members = archive.namelist()
            if any(members.count(key + '.npy') != 1 for key in required):
                raise ValueError('selection_changed')
            for key, kinds in required.items():
                info = archive.getinfo(key + '.npy')
                with archive.open(info) as member:
                    version = np.lib.format.read_magic(member)
                    readers = {(1, 0): np.lib.format.read_array_header_1_0,
                               (2, 0): np.lib.format.read_array_header_2_0}
                    if version not in readers:
                        raise ValueError('selection_changed')
                    shape, _, dtype = readers[version](member)
                    size = length * dtype.itemsize
                    if (shape != (length,) or dtype.kind not in kinds
                            or info.file_size - member.tell() != size):
                        raise ValueError('selection_changed')
                    if size == 0:
                        arrays[key] = np.empty(shape, dtype=dtype)
                        continue
                    with tempfile.TemporaryFile() as backing:
                        shutil.copyfileobj(member, backing, length=1024 * 1024)
                        if backing.tell() != size:
                            raise ValueError('selection_changed')
                        backing.flush()
                        arrays[key] = np.memmap(backing, dtype=dtype, mode='r', shape=shape)
    except (zipfile.BadZipFile, EOFError, TypeError, ValueError) as error:
        raise ValueError('selection_changed') from error
    return arrays


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
            if not isinstance(definition, dict) or len(definition.get('json', '').encode('utf-8')) > 65536 or hashlib.sha256(definition.get('json', '').encode('utf-8')).hexdigest() != definition.get('sha256'):
                raise ValueError('invalid_export')
            payload = json.loads(definition['json'])
            observation = payload['alignment']['observation']
            if payload['sourceSha256'] != manifest['source']['sha256'] or observation['qcId'] != expected['id'] or observation['manifestSha256'] != expected['manifestSha256'] or payload['metadata']['donorColumn'] != manifest['qc']['options']['donorColumn']:
                raise ValueError('selection_changed')
            data = open_research_h5ad(source)
            try:
                selected = scratch_vector(data.n_obs, np.bool_)
                selected[:] = True
                for role, single, many in (('conditionColumn', payload['conditionValue'], payload.get('conditionValues', [])), ('perturbationColumn', payload['perturbationValue'], payload.get('perturbationValues', []))):
                    if not isinstance(many, list) or (many and (not 2 <= len(many) <= 20 or any(not isinstance(v, str) or not v.strip() or len(v.encode('utf-8')) > 200 or '\0' in v for v in many) or many != sorted(set(many)) or many[0] != single)):
                        raise ValueError('invalid_export')
                    values = many or [single]
                    column = payload['metadata'][role]
                    if not column or column not in data.obs or not isinstance(single, str) or not single.strip():
                        raise ValueError('invalid_export')
                    for start, block in iter_annotation_column(data.obs, column, include_index=False):
                        selected[start:start + len(block)] &= block.astype('string').isin(values).fillna(False).to_numpy(dtype=bool)
            finally:
                data.file.close()
            matched = int(selected.sum())
            row_digest = hashlib.sha256()
            parent_retained = arrays['retained']
            for start in range(0, len(selected), 8192):
                block = slice(start, start + 8192)
                selected[block] &= parent_retained[block]
                row_digest.update(arrays['source_row'][block][selected[block]].astype('<i8').tobytes())
            if not selected.any():
                raise ValueError('empty_selection')
            arrays['retained'] = selected
            mappings = {m['label']: m for m in payload['alignment']['settings']['mappings']}
            arrays['canonical_donor_id'] = DonorExportColumn(arrays['donor_id'], mappings)
            arrays['canonical_donor_reviewed'] = DonorExportColumn(arrays['donor_id'], mappings, reviewed=True)
            manifest['cohort'] = {'definition_json': definition['json'], 'definition_sha256': definition['sha256'], 'matched': matched, 'retained': int(arrays['retained'].sum()), 'source_rows_sha256': row_digest.hexdigest(), 'selection_rule': 'exact-value OR within each role; condition AND perturbation AND parent QC retained; missing values excluded'}
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

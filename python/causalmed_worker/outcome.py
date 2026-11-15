"""Read-only single-feature observations; no predictions or cell-level IPC."""
import hashlib
import json
from pathlib import Path

if 'open_research_h5ad' not in globals():
    from causalmed_worker.backed import open_research_h5ad, iter_frame_index, iter_annotation_column, index_matches


def match_feature_space(frame, column, target, row):
    """Match in source order and hash the historical sorted JSON representation."""
    import sqlite3
    from contextlib import closing

    blocks = iter_frame_index(frame) if column == '_index' else iter_annotation_column(frame, column, include_index=False)
    position = None
    count = matched = 0
    invalid = False
    with closing(sqlite3.connect('')) as database:
        database.execute('PRAGMA cache_size=-1024')
        database.execute('CREATE TABLE features (id TEXT PRIMARY KEY) WITHOUT ROWID')
        for start, block in blocks:
            identifiers = block.astype('string')
            for offset, identifier in enumerate(identifiers):
                if not isinstance(identifier, str) or not identifier.strip():
                    invalid = True
                if isinstance(identifier, str) and identifier == target:
                    position = start + offset
                    matched += 1
            count += len(identifiers)
            database.executemany('INSERT OR IGNORE INTO features VALUES (?)',
                                 ((value,) for value in identifiers if isinstance(value, str)))
        row['matchedFeatures'] = matched
        row['featureCount'] = count
        if matched != 1:
            raise ValueError('feature_missing' if not matched else 'feature_ambiguous')
        if invalid or database.execute('SELECT count(*) FROM features').fetchone()[0] != count:
            raise ValueError('feature_space_invalid')
        digest = hashlib.sha256(b'[')
        separator = b''
        for (identifier,) in database.execute('SELECT id FROM features ORDER BY id COLLATE BINARY'):
            digest.update(separator)
            digest.update(json.dumps(identifier, ensure_ascii=False).encode('utf-8'))
            separator = b','
        digest.update(b']')
        row['featureSpaceSha256'] = digest.hexdigest()
    return position


def inspect_outcome(context, context_sha, jobs, progress=lambda p, s: None):
    import numpy as np
    from scipy import sparse
    if 'load_selection' not in globals():
        from causalmed_worker.selection import load_selection, file_digest
    else:
        load_selection = globals()['load_selection']
        file_digest = globals()['file_digest']
    # Bound progress traffic even for millions of rows.
    send_progress = progress
    last_percent = [-1]
    def progress(percent, phase):
        if percent != last_percent[0]:
            last_percent[0] = percent
            send_progress(percent, phase)
    results = []
    for index, source in enumerate(context['inputs']):
        obs = source['cohort']['observation']
        row = dict(sourceSha256=source['sourceSha256'], qcId=obs['qcId'], matrix='', featureCount=0,
                   featureSpaceSha256=None, matchedFeatures=0, retained=obs['retained'],
                   missingDonorCells=obs['missingRetained'], issues=[], donors=[])
        data = None
        try:
            job = jobs[index]
            if job.get('issue'):
                raise ValueError(job['issue'])
            progress(int(index * 95 / max(len(jobs), 1)), 'checksum')
            path = Path(job['path'])
            try:
                before = path.stat()
                if not path.is_file() or file_digest(path) != source['sourceSha256']:
                    raise ValueError('file_changed')
            except OSError:
                raise ValueError('file_changed') from None
            try:
                manifest, arrays = load_selection(job['selectionDir'], job['selection'])
            except OSError:
                raise ValueError('selection_changed') from None
            if manifest['analysisId'] != obs['qcId'] or manifest['source']['sha256'] != source['sourceSha256'] or job['selection']['manifestSha256'] != obs['manifestSha256']:
                raise ValueError('selection_changed')
            data = open_research_h5ad(path)
            if not index_matches(data.obs, arrays['cell_id']):
                raise ValueError('selection_changed')
            key = manifest['qc']['options']['matrix']
            row['matrix'] = key
            # Read HDF5 matrices directly, including layers; AnnData backed layers may materialize.
            from anndata.io import sparse_dataset
            import h5py
            handle = data.file._file
            node = handle['raw/X'] if key == 'raw' else handle['layers'][key[6:]] if key.startswith('layer:') else handle['X']
            matrix = sparse_dataset(node) if isinstance(node, h5py.Group) else node
            var = data.raw.var if key == 'raw' else data.var
            column = context['spec']['featureColumn']
            feature_position = match_feature_space(var, column, context['spec']['featureId'], row)
            kept = np.asarray(arrays['retained'], dtype=bool)
            if int(kept.sum()) != obs['retained'] or int((kept & arrays['donor_missing']).sum()) != obs['missingRetained']:
                raise ValueError('selection_changed')
            if not kept.any():
                raise ValueError('empty_cohort')
            if obs['missingRetained']:
                raise ValueError('missing_donors')
            labels = arrays['donor_id']
            sums, counts = {}, {}
            for start in range(0, data.n_obs, 8192):
                end = min(start + 8192, data.n_obs)
                mask = kept[start:end]
                if not mask.any():
                    continue
                values = matrix[start:end, feature_position:feature_position + 1]
                values = values.toarray() if sparse.issparse(values) else np.asarray(values)
                values = values.reshape(-1)[mask].astype(np.float64)
                totals = arrays['total_counts'][start:end][mask]
                if not np.isfinite(totals).all() or (totals <= 0).any():
                    raise ValueError('invalid_library')
                if not np.isfinite(values).all() or (values < 0).any() or not np.equal(values, np.floor(values)).all() or (values > totals + 1e-6).any():
                    raise ValueError('invalid_expression')
                normalized = np.log1p(values / totals * 10000)
                current = labels[start:end][mask]
                for label in np.unique(current):
                    selected = current == label
                    sums[str(label)] = sums.get(str(label), 0.) + float(normalized[selected].sum())
                    counts[str(label)] = counts.get(str(label), 0) + int(selected.sum())
                progress(int((index + end / max(data.n_obs, 1)) * 95 / max(len(jobs), 1)), 'expression')
            expected = {d['label']: d['retained'] for d in obs['donors'] if d['retained'] > 0}
            if counts != expected:
                raise ValueError('selection_changed')
            after = path.stat()
            if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
                raise ValueError('file_changed')
            row['donors'] = [dict(label=label, cells=counts[label], mean=sums[label]/counts[label]) for label in sorted(counts)]
        except (OSError, KeyError, ValueError, TypeError) as error:
            allowed = {'file_changed', 'selection_changed', 'feature_missing', 'feature_ambiguous', 'feature_space_invalid', 'empty_cohort', 'missing_donors', 'invalid_library', 'invalid_expression', 'matrix_unavailable'}
            code = str(error)
            row['issues'] = [code if code in allowed else 'matrix_unavailable']
            row['donors'] = []
        finally:
            if data is not None:
                data.file.close()
        results.append(row)
    return dict(version=1, contextSha256=context_sha, inputs=results)

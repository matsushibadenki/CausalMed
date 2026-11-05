"""Dependency-free reference worker for reproducible, donor-weighted M0 checks."""
import argparse
from collections import defaultdict
import hashlib
import json
import math
from pathlib import Path
import sys


def evaluate(samples, held_out_dataset, held_out_perturbation):
    ids = set()
    for sample in samples:
        for key in ('id', 'dataset', 'donor', 'perturbation', 'condition'):
            if not isinstance(sample.get(key), str) or not sample[key].strip():
                raise ValueError('invalid_samples')
        value = sample.get('value')
        if type(value) not in (int, float) or not math.isfinite(value) or sample['id'] in ids:
            raise ValueError('invalid_samples')
        ids.add(sample['id'])
    if not samples:
        raise ValueError('invalid_samples')
    test = [s for s in samples if s['dataset'] == held_out_dataset and s['perturbation'] == held_out_perturbation]
    test_donors = {s['donor'] for s in test}
    train = [s for s in samples if s['dataset'] != held_out_dataset and s['perturbation'] != held_out_perturbation and s['donor'] not in test_donors]
    conditions = {s['condition'] for s in test}
    controls = [s for s in train if s['perturbation'] == 'control' and s['condition'] in conditions]

    def donor_means(rows):
        groups = defaultdict(list)
        for row in rows:
            groups[row['donor']].append(row['value'])
        return [sum(values) / len(values) for values in groups.values()]

    control_means, test_means = donor_means(controls), donor_means(test)
    reason = None
    if len(conditions) != 1:
        reason = 'single_condition_required'
    elif len(control_means) < 2 or len(test_means) < 2:
        reason = 'insufficient_independent_donors'
    mean = sum(control_means) / len(control_means) if control_means else 0.0
    mae = sum(abs(value - mean) for value in test_means) / len(test_means) if not reason else 0.0
    return dict(model='M0-control-mean-v1', heldOutDataset=held_out_dataset,
                heldOutPerturbation=held_out_perturbation, trainIds=[s['id'] for s in train],
                testIds=[s['id'] for s in test], trainDonors=len(control_means), testDonors=len(test_means),
                controlMean=mean, donorMae=mae, abstained=reason is not None, reason=reason)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--samples', required=True, type=Path)
    parser.add_argument('--held-out-dataset', required=True)
    parser.add_argument('--held-out-perturbation', required=True)
    parser.add_argument('--synthetic', action='store_true', help='Explicitly mark synthetic input.')
    args = parser.parse_args()
    try:
        if args.samples.stat().st_size > 2 * 1024 * 1024:
            raise ValueError('input_too_large')
        raw = args.samples.read_bytes()
        report = evaluate(json.loads(raw), args.held_out_dataset, args.held_out_perturbation)
        report.update(datasetSha256=hashlib.sha256(raw).hexdigest(), synthetic=args.synthetic)
        print(json.dumps(report, allow_nan=False, indent=2))
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(json.dumps({'error': str(error)}), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())

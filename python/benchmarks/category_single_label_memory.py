"""Compare whole-process RSS for one VLEN category label in fresh processes."""
import argparse
import json
from pathlib import Path
import resource
from statistics import median
import subprocess
import sys
import tempfile
from unittest.mock import patch

import h5py
from causalmed_worker import backed


def measure(path, cache_kib):
    import pandas  # Include the normal worker's pandas import in the baseline.

    def validate_with_cache(files, validate):
        file, = files
        original = file.id.get_mdc_config()
        config = file.id.get_mdc_config()
        config.min_size = config.max_size = config.initial_size = cache_kib * 1024
        config.set_initial_size = True
        file.id.set_mdc_config(config)
        try:
            validate()
        finally:
            file.id.set_mdc_config(original)

    with h5py.File(path, 'r') as source:
        dictionary = backed.CategoryDictionary(source['labels'])
        if cache_kib == 1024:
            dictionary.validate()
        else:
            with patch.object(backed, 'with_small_metadata_caches', validate_with_cache):
                dictionary.validate()
    scale = 1 if sys.platform == 'darwin' else 1024
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale / 1024**2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--compare-cache-kib', type=int, default=256)
    parser.add_argument('--measure', type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.measure is not None:
        print(measure(args.measure, args.compare_cache_kib))
        return
    if args.runs < 1 or args.compare_cache_kib < 1:
        parser.error('runs and cache size must be positive')

    caches = (1024, args.compare_cache_kib)
    results = {size: [] for size in caches}
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / 'single-label.h5'
        with h5py.File(path, 'w') as source:
            # A single 6 MiB UTF-8 payload forces exactly one scalar read.
            source['labels'] = ['🧬' * (1536 * 1024)]
        for _ in range(args.runs):
            for size in results:
                output = subprocess.check_output(
                    [sys.executable, __file__, '--measure', str(path), '--compare-cache-kib', str(size)],
                    text=True,
                )
                results[size].append(float(output))
    print(json.dumps({
        'hdf5': h5py.version.hdf5_version,
        'peakRssMiB': {size: {'samples': values, 'median': median(values)}
                       for size, values in results.items()},
    }, indent=2))


if __name__ == '__main__':
    main()

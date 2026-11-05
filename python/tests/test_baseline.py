import copy
import json
from pathlib import Path
import unittest
from causalmed_worker.__main__ import evaluate

FIXTURE = Path(__file__).parents[2] / 'fixtures' / 'benchmark.synthetic.json'


class BaselineTests(unittest.TestCase):
    def setUp(self):
        self.samples = json.loads(FIXTURE.read_text())

    def run_model(self):
        return evaluate(self.samples, 'held-out', 'GENE-E')

    def test_known_scalar_baseline(self):
        report = self.run_model()
        self.assertFalse(report['abstained'])
        self.assertAlmostEqual(report['controlMean'], 1.2)
        self.assertAlmostEqual(report['donorMae'], 0.6)
        self.assertFalse(set(report['trainIds']) & set(report['testIds']))

    def test_test_donor_cannot_leak_from_another_dataset(self):
        contaminated = copy.deepcopy(self.samples[-1])
        contaminated.update(id='leak', dataset='training', perturbation='control', value=1000)
        self.samples.append(contaminated)
        self.assertNotIn('leak', self.run_model()['trainIds'])
        self.assertAlmostEqual(self.run_model()['controlMean'], 1.2)

    def test_more_cells_do_not_inflate_donor_weight(self):
        extra = [s for s in self.samples if s['donor'] == 'D1']
        for i in range(50):
            self.samples += [dict(s, id=f'extra-{i}-{s["id"]}') for s in extra]
        self.assertAlmostEqual(self.run_model()['controlMean'], 1.2)

    def test_abstains_with_one_donor(self):
        self.samples = [s for s in self.samples if s['donor'] != 'D4']
        self.assertEqual(self.run_model()['reason'], 'insufficient_independent_donors')

    def test_rejects_nonfinite_or_duplicate_input(self):
        self.samples[0]['value'] = float('nan')
        with self.assertRaises(ValueError):
            self.run_model()


if __name__ == '__main__':
    unittest.main()

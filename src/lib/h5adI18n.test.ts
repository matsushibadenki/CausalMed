import { expect, it } from 'vitest';
import { h5adText } from './h5adI18n';
it('has translated lifecycle, error and phase labels for every supported locale', () => {
  const required = ['cancelled', 'timed_out', 'cancelling', 'runtime', 'checksum', 'opening', 'annotations', 'validating', 'python_dependencies_missing', 'worker_protocol_error'] as const;
  for (const language of Object.values(h5adText)) {
    expect(Object.keys(language).sort()).toEqual(Object.keys(h5adText.en).sort());
    for (const key of required) expect(language[key].length).toBeGreaterThan(0);
  }
});

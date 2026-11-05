import { describe, expect, it } from 'vitest';
import { errorMessage, translator } from './i18n';

describe('research status language',()=>{
  it.each(['en','ja','zh'] as const)('keeps observation, intervention and hypothesis distinct in %s',locale=>{
    const t=translator(locale);
    expect(new Set(['observation','intervention','hypothesis'].map(t)).size).toBe(3);
    expect(t('safetyUnknown')).not.toBe('safetyUnknown');
    expect(t('unvalidated')).not.toBe('unvalidated');
    expect(t('abstained')).not.toBe('abstained');
  });
  it('maps a structured Rust rejection without exposing storage details',()=>{
    expect(errorMessage('observation_cannot_assert_causality','ja')).toContain('観測');
    expect(errorMessage('storage_error: /private/user/file.sqlite3','en')).not.toContain('/private');
    expect(errorMessage(new Error('desktop_required'),'en')).toContain('desktop');
  });
});

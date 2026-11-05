import { bulkText } from './bulkI18n';
import { trashText } from './trashI18n';
import { browseText } from './browseI18n';
import { projectText } from './projectI18n';
import { historyText } from './historyI18n';
import { exportText } from './exportI18n';
import { qcText } from './qcI18n';
import { expect, it } from 'vitest';
import { h5adText } from './h5adI18n';
it('has translated lifecycle, error and phase labels for every supported locale', () => {
  const required = ['cancelled', 'timed_out', 'cancelling', 'runtime', 'checksum', 'opening', 'annotations', 'validating', 'python_dependencies_missing', 'worker_protocol_error'] as const;
  for (const language of Object.values(h5adText)) {
    expect(Object.keys(language).sort()).toEqual(Object.keys(h5adText.en).sort());
    for (const key of required) expect(language[key].length).toBeGreaterThan(0);
  }
});

it('includes QC thresholds, mapping errors and partial states in every locale',()=>{
  for (const locale of Object.values(qcText)) {
    expect(Object.keys(locale).sort()).toEqual(Object.keys(qcText.en).sort());
    for (const value of Object.values(locale)) expect(value.length).toBeGreaterThan(0);
  }
});

it('includes export outcomes and integrity errors in every locale',()=>{
  for (const locale of Object.values(exportText)) {
    expect(Object.keys(locale).sort()).toEqual(Object.keys(exportText.en).sort());
    for (const value of Object.values(locale)) expect(value.length).toBeGreaterThan(0);
  }
});

it('includes QC history and comparison labels in every locale',()=>{
 for (const locale of Object.values(historyText)) {
  expect(Object.keys(locale).sort()).toEqual(Object.keys(historyText.en).sort());
  for (const value of Object.values(locale)) expect(value.length).toBeGreaterThan(0);
 }
});

it('includes project and source-relink labels in every locale',()=>{
 for(const locale of Object.values(projectText)){expect(Object.keys(locale).sort()).toEqual(Object.keys(projectText.en).sort());for(const value of Object.values(locale))expect(value.length).toBeGreaterThan(0);}
});

it('includes search/filter/sort/pagination labels in every locale',()=>{
 for(const locale of Object.values(browseText)){expect(Object.keys(locale).sort()).toEqual(Object.keys(browseText.en).sort());for(const value of Object.values(locale))expect(value.length).toBeGreaterThan(0);}
});

it('includes reversible trash actions and errors in every locale',()=>{for(const locale of Object.values(trashText)){expect(Object.keys(locale).sort()).toEqual(Object.keys(trashText.en).sort());for(const value of Object.values(locale))expect(value.length).toBeGreaterThan(0);}});

it('includes bulk selection and outcomes in every locale',()=>{for(const locale of Object.values(bulkText)){expect(Object.keys(locale).sort()).toEqual(Object.keys(bulkText.en).sort());for(const value of Object.values(locale))expect(value.length).toBeGreaterThan(0);}});

import { useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { Check, Database, FileUp, LoaderCircle, Square, Timer, TriangleAlert } from 'lucide-react';
import type { Locale } from '../lib/types';
import { cancelH5ad, startH5ad, useH5ad, workerActive } from '../lib/h5ad';
import { h5adText, type H5adTextKey } from '../lib/h5adI18n';
import { Info, IconButton } from './ui';
const phases = ['runtime', 'checksum', 'opening', 'annotations', 'validating'] as const;
const colors = ['#2a8285', '#516b9c', '#b66339', '#82749b', '#63806a', '#ac8343', '#758591', '#a06772'];
export function H5adImport({ locale }: { locale: Locale }) {
  const text = h5adText[locale];
  const job = useH5ad();
  const report = job.report;
  const active = workerActive(job.status);
  const [timeout, setTimeoutSeconds] = useState(120);
  const [pending, setPending] = useState(false);
  const [localError, setLocalError] = useState('');
  const [selectedGroup, setSelectedGroup] = useState('');
  const keys = Object.keys(report?.groups ?? {});
  const groupKey = keys.includes(selectedGroup) ? selectedGroup : keys[0];
  const group = report?.groups[groupKey];
  const t = (key: string) => text[key as H5adTextKey] ?? text.worker_failed;
  const terminal = !['idle', 'selecting', 'running', 'cancelling'].includes(job.status);
  const warning = ['failed', 'timed_out'].includes(job.status);
  async function run() {
    setPending(true); setLocalError('');
    try { await startH5ad(timeout); } catch (error) { setLocalError(String(error)); }
    finally { setPending(false); }
  }
  return <section className="h5ad-panel" aria-labelledby="h5ad-heading">
    <header className="h5ad-header"><div><Database size={19}/><h2 id="h5ad-heading">{text.title}</h2><Info text={text.help}/></div>
      <div className="h5ad-actions"><label className="h5ad-timeout"><Timer size={15}/><span className="sr-only">{text.timeout}</span><select aria-label={text.timeout} value={timeout} disabled={active || pending} onChange={e => setTimeoutSeconds(Number(e.target.value))}>{[1, 30, 120, 600, 3600].map(seconds => <option key={seconds} value={seconds}>{seconds} {text.seconds}</option>)}</select></label>
        <button className="button" disabled={!isTauri() || active || pending} onClick={() => void run()}><FileUp size={15}/>{text.import}</button>
      </div>
    </header>
    {(active || terminal || localError) ? <div className={`h5ad-job ${warning || localError ? 'has-warning' : ''}`}>
      <div className="h5ad-job-line"><span role="status" aria-live="polite">{active ? <LoaderCircle size={15} className="spin"/> : warning || localError ? <TriangleAlert size={15}/> : <Check size={15}/>} {localError ? t(localError) : t(job.status)}{job.status === 'running' ? ` · ${t(job.phase)}` : ''}</span><div><time>{(job.elapsedMs / 1000).toFixed(1)} / {job.timeoutSeconds} {text.seconds}</time>{active && job.status !== 'selecting' ? <IconButton label={text.cancel} disabled={job.status === 'cancelling'} onClick={() => void cancelH5ad().catch(() => setLocalError('worker_failed'))}><Square size={13}/></IconButton> : null}</div></div>
      {active ? <><progress aria-label={text.running} max={100} value={job.percent}/><div className="h5ad-phases">{phases.map((phase, index) => <span key={phase} className={index <= phases.indexOf(job.phase as typeof phases[number]) ? 'reached' : ''}>{t(phase)}</span>)}<b>{job.percent}%</b></div></> : null}
      {job.error ? <p role="alert">{t(job.error)}</p> : null}
      {report && ['cancelled', 'timed_out', 'failed'].includes(job.status) ? <small>{text.retained}</small> : null}
    </div> : null}
    {report ? <div className="h5ad-report">
      <div className="h5ad-file-heading"><h3>{report.name}</h3><span className="neutral-label">{text.unknown}</span></div>
      <dl className="h5ad-metrics"><div><dt>{text.cells}</dt><dd>{report.cells.toLocaleString()}</dd></div><div><dt>{text.genes}</dt><dd>{report.genes.toLocaleString()}</dd></div><div><dt>{text.size}</dt><dd>{report.bytes ? `${(report.bytes / 1024 / 1024).toFixed(1)} MB` : '—'}</dd></div><div className={report.duplicateCellIds + report.duplicateGeneIds > 0 ? 'metric-warning' : ''}><dt>{text.duplicates}</dt><dd>{(report.duplicateCellIds + report.duplicateGeneIds).toLocaleString()}</dd></div></dl>
      {group ? <section className="h5ad-distribution"><div className="h5ad-distribution-heading"><label>{text.grouping} <select value={groupKey} onChange={e => setSelectedGroup(e.target.value)}>{keys.map(key => <option key={key}>{key}</option>)}</select></label><span>{group.distinct} · {text.missing} {group.missing}</span></div>
        <div className="h5ad-stacked" aria-hidden="true">{group.counts.map((row, index) => <span key={index} style={{ width: `${row.count / report.cells * 100}%`, background: colors[index % colors.length] }}/>) }{group.missing + (group.other ?? 0) > 0 ? <span style={{flex: 1, background: '#b8b7b1'}}/> : null}</div>
        <div className="h5ad-category-grid">{[...group.counts, ...(group.other ? [{label: text.other, count: group.other}] : []), ...(group.missing ? [{label: text.missing, count: group.missing}] : [])].map((row, index) => <div className="h5ad-category" key={index}><span className="h5ad-category-dot" style={{background: colors[index % colors.length]}}/><span>{row.label}</span><b>{row.count.toLocaleString()}</b><span className="h5ad-percentage">{(row.count / report.cells * 100).toFixed(1)}%</span></div>)}</div>
      </section> : <p className="source-note">{text.noGroups}</p>}
      <div className="h5ad-footer"><span>{text.metadata}</span>{!report.obsColumns.some(c => ['donor', 'donor_id'].includes(c)) ? <span><TriangleAlert size={13}/>{text.donorAbsent}</span> : null}<details><summary>{text.obs} · SHA-256</summary><p>{text.columns}: {report.obsColumns.join(', ') || '—'}</p><code>{report.sha256}</code><p>{text.version}: anndata {report.anndataVersion ?? '—'}</p></details></div>
    </div> : <div className="h5ad-empty"><FileUp size={25} strokeWidth={1.3}/><span>{text.empty}</span></div>}
  </section>;
}

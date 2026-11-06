import { resourceText } from '../lib/resourceI18n';
import { SourceRelink } from './SourceRelink';
import { projectText } from '../lib/projectI18n';
import { historyText } from '../lib/historyI18n';
import { QcHistory } from './QcHistory';
import { exportText } from '../lib/exportI18n';
import { ExpressionQC } from './ExpressionQC';
import { qcText } from '../lib/qcI18n';
import { useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { Check, Database, FileUp, LoaderCircle, Square, Timer, TriangleAlert } from 'lucide-react';
import type { Locale } from '../lib/types';
import { cancelH5ad, startH5ad, useH5ad, workerActive } from '../lib/h5ad';
import { h5adText, type H5adTextKey } from '../lib/h5adI18n';
import { Info, IconButton } from './ui';
const phases = ['runtime', 'checksum', 'opening', 'annotations', 'expression', 'validating'] as const;
const colors = ['#2a8285', '#516b9c', '#b66339', '#82749b', '#63806a', '#ac8343', '#758591', '#a06772'];
export function H5adImport({ locale, sourceSha256='' }: { locale: Locale;sourceSha256?:string }) {
  const text = h5adText[locale];
  const job = useH5ad();
  const report = job.report;
  const active = workerActive(job.status);
  const exporting = job.operation.startsWith('export_');
  const comparing = job.operation === 'compare';
  const relinking = job.operation === 'relink';
  const pt = projectText[locale];
  const ht = historyText[locale];
  const xt = exportText[locale];
  const steps = relinking ? ['runtime','checksum'] : comparing ? ['runtime','selection'] : exporting ? ['selection','checksum','writing','export_verify'] : [...phases,'selection'];
  const [timeout, setTimeoutSeconds] = useState(120);
  const [pending, setPending] = useState(false);
  const [localError, setLocalError] = useState('');
  const [selectedGroup, setSelectedGroup] = useState('');
  const keys = Object.keys(report?.groups ?? {});
  const groupKey = keys.includes(selectedGroup) ? selectedGroup : keys[0];
  const group = report?.groups[groupKey];
  const t = (key: string) => resourceText[locale][key as keyof typeof resourceText.en] ?? text[key as H5adTextKey] ?? xt[key as keyof typeof xt] ?? qcText[locale][key as keyof typeof qcText.en] ?? text.worker_failed;
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
      <div className="h5ad-job-line"><span role="status" aria-live="polite">{active ? <LoaderCircle size={15} className="spin"/> : warning || localError ? <TriangleAlert size={15}/> : <Check size={15}/>} {localError ? t(localError) : (relinking ? ({running:pt.relinking,completed:pt.relinked,failed:pt.relinkFailed,cancelled:pt.relinkCancelled}[job.status]??t(job.status)) : comparing && ['running','completed'].includes(job.status) ? ht[job.status as 'running'|'completed'] : exporting && ['running','completed','failed','cancelled'].includes(job.status) ? xt[job.status as 'running'|'completed'|'failed'|'cancelled'] : t(job.status))}{job.status === 'running' ? ` · ${t(job.phase)}` : ''}</span><div><time>{(job.elapsedMs / 1000).toFixed(1)} / {job.timeoutSeconds} {text.seconds}</time>{active && job.status !== 'selecting' ? <IconButton label={relinking?pt.cancel:comparing?ht.cancel:exporting?xt.cancel:text.cancel} disabled={job.status === 'cancelling'} onClick={() => void cancelH5ad().catch(() => setLocalError('worker_failed'))}><Square size={13}/></IconButton> : null}</div></div>
      {active ? <><progress aria-label={relinking?pt.relinking:comparing?ht.running:exporting?xt.running:text.running} max={100} value={job.percent}/><div className="h5ad-phases">{steps.map((phase, index) => <span key={phase} className={index <= steps.indexOf(job.phase) ? 'reached' : ''}>{t(phase)}</span>)}<b>{job.percent}%</b></div></> : null}
      {job.error ? <p role="alert">{relinking && job.error==='file_changed'?pt.file_changed:t(job.error)}</p> : null}
      {report && ['cancelled', 'timed_out', 'failed'].includes(job.status) ? <small>{exporting?xt.noOutput:text.retained}</small> : null}
    </div> : null}
    <SourceRelink key={report?.selection?.id} locale={locale} timeout={timeout}/>
    <QcHistory locale={locale} job={job} active={active} sourceSha256={sourceSha256}/>
    {report ? <div className="h5ad-report">
      <div className="h5ad-file-heading"><h3>{report.name}</h3><span className="neutral-label">{text.unknown}</span></div>
      <dl className="h5ad-metrics"><div><dt>{text.cells}</dt><dd>{report.cells.toLocaleString()}</dd></div><div><dt>{text.genes}</dt><dd>{report.genes.toLocaleString()}</dd></div><div><dt>{text.size}</dt><dd>{report.bytes ? `${(report.bytes / 1024 / 1024).toFixed(1)} MB` : '—'}</dd></div><div className={report.duplicateCellIds + report.duplicateGeneIds > 0 ? 'metric-warning' : ''}><dt>{text.duplicates}</dt><dd>{(report.duplicateCellIds + report.duplicateGeneIds).toLocaleString()}</dd></div></dl>
      {group ? <section className="h5ad-distribution"><div className="h5ad-distribution-heading"><label>{text.grouping} <select value={groupKey} onChange={e => setSelectedGroup(e.target.value)}>{keys.map(key => <option key={key}>{key}</option>)}</select></label><span>{group.distinct} · {text.missing} {group.missing}</span></div>
        <div className="h5ad-stacked" aria-hidden="true">{group.counts.map((row, index) => <span key={index} style={{ width: `${row.count / report.cells * 100}%`, background: colors[index % colors.length] }}/>) }{group.missing + (group.other ?? 0) > 0 ? <span style={{flex: 1, background: '#b8b7b1'}}/> : null}</div>
        <div className="h5ad-category-grid">{[...group.counts, ...(group.other ? [{label: text.other, count: group.other}] : []), ...(group.missing ? [{label: text.missing, count: group.missing}] : [])].map((row, index) => <div className="h5ad-category" key={index}><span className="h5ad-category-dot" style={{background: colors[index % colors.length]}}/><span>{row.label}</span><b>{row.count.toLocaleString()}</b><span className="h5ad-percentage">{(row.count / report.cells * 100).toFixed(1)}%</span></div>)}</div>
      </section> : <p className="source-note">{text.noGroups}</p>}
      <ExpressionQC key={report.selection?.id ?? report.sha256 + JSON.stringify(report.qc?.options)} report={report} locale={locale} active={active} canAnalyze={job.canAnalyze} timeout={timeout}/>
      <div className="h5ad-footer"><span>{text.metadata}</span>{!report.qc?.donor.column && !report.obsColumns.some(c => ['donor', 'donor_id'].includes(c)) ? <span><TriangleAlert size={13}/>{text.donorAbsent}</span> : null}<details><summary>{text.obs} · SHA-256</summary><p>{text.columns}: {report.obsColumns.join(', ') || '—'}</p><code>{report.sha256}</code><p>{text.version}: anndata {report.anndataVersion ?? '—'}</p></details></div>
    </div> : <div className="h5ad-empty"><FileUp size={25} strokeWidth={1.3}/><span>{text.empty}</span></div>}
  </section>;
}

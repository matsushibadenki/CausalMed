import { useState } from 'react';
import { Archive, Check, FileOutput } from 'lucide-react';
import { exportH5adSelection, useH5ad } from '../lib/h5ad';
import type { H5adReport } from '../lib/h5ad';
import type { Locale } from '../lib/types';
import { exportText } from '../lib/exportI18n';
import { qcText } from '../lib/qcI18n';
import { formatDate } from '../lib/i18n';
import { Info } from './ui';
export function QcExports({ report, locale, active, dirty, timeout }: {report:H5adReport;locale:Locale;active:boolean;dirty:boolean;timeout:number}) {
  const t = exportText[locale];
  const selection = report.selection;
  const { lastExport, sourceLinked } = useH5ad();
  const [pending, setPending] = useState(false), [error, setError] = useState('');
  const disabled = active || pending || dirty || !selection;
  async function run(kind:'export_qc'|'export_h5ad') {
    if (!selection) return;
    setPending(true); setError('');
    try { await exportH5adSelection(selection.id,kind,timeout); }
    catch (error) { setError(String(error)); }
    finally { setPending(false); }
  }
  const message = error ? t[error as keyof typeof t] ?? qcText[locale][error as keyof typeof qcText.en] ?? t.failed : '';
  return <div className="qc-exports">
    <div className="qc-export-status">{selection ? <><Check size={14}/><span>{t.saved}</span><code title={selection.id}>{selection.id.slice(0,8)}</code><time>{formatDate(selection.createdAt,locale)}</time></> : <span>{t.rerun}</span>}<Info text={t.help}/></div>
    <div className="qc-export-actions"><button className="button" type="button" disabled={disabled} onClick={()=>void run('export_qc')}><Archive size={14}/>{t.saveQc}</button><button className="button" type="button" disabled={disabled || !selection?.retained || !sourceLinked} title={selection?.retained===0?t.empty_selection:undefined} onClick={()=>void run('export_h5ad')}><FileOutput size={14}/>{t.saveH5ad}</button></div>
    {dirty ? <p className="source-note">{t.dirty}</p> : null}
    {message ? <p role="alert" className="form-error">{message}</p> : null}
    {lastExport && lastExport.selectionId===selection?.id ? <details className="qc-export-receipt"><summary>{t.lastExport}: {lastExport.fileName}</summary><code>SHA-256 {lastExport.sha256}</code></details> : null}
  </div>;
}

import { QcExports } from './QcExports';
import { useState } from 'react';
import { Play, SlidersHorizontal, Users, TriangleAlert } from 'lucide-react';
import type { Locale } from '../lib/types';
import type { H5adReport, Histogram, QcOptions } from '../lib/h5ad';
import { analyzeH5ad } from '../lib/h5ad';
import { qcText } from '../lib/qcI18n';
import { Info } from './ui';
const defaults: QcOptions = { matrix:'X', geneColumn:'_index', mitoPrefix:'MT-', donorColumn:'', minGenes:200, maxGenes:6000, minCounts:0, maxCounts:null, maxMitoPct:20 };
function HistogramPlot({histogram, title, median, logLabel, cells}:{histogram:Histogram;title:string;median:string;logLabel:string;cells:string}) {
  const { counts, edges } = histogram;
  const transform = (value:number) => histogram.log ? Math.log10(1 + value) : value;
  const lo = transform(edges[0]), hi = transform(edges[edges.length-1]);
  const x = (value:number) => 12 + Math.max(0, Math.min(1, (transform(value)-lo)/(hi-lo || 1))) * 276;
  const peak = Math.max(...counts,1);
  const format = (value:number) => value.toLocaleString(undefined,{maximumFractionDigits:histogram.log ? 1 : 2});
  return <article className="qc-chart"><div><h4>{title}</h4><span>{median} <b>{format(histogram.median)}</b></span></div>
    <svg viewBox="0 0 300 122" role="img" aria-label={`${title}: ${median} ${format(histogram.median)}; ${counts.reduce((a,b)=>a+b,0)} ${cells}`}>
      {histogram.lower !== null ? <rect x="12" y="10" width={Math.max(0,x(histogram.lower)-12)} height="82" fill="#b6633918"/> : null}
      {histogram.upper !== null ? <rect x={x(histogram.upper)} y="10" width={288-x(histogram.upper)} height="82" fill="#b6633918"/> : null}
      {counts.map((count,index)=><rect key={index} x={12 + index*276/counts.length + 1} y={92-count/peak*76} width={276/counts.length-2} height={count/peak*76} rx="1" fill="#2a8285"><title>{format(edges[index])}–{format(edges[index+1])}: {count.toLocaleString()} {cells}</title></rect>)}
      {[histogram.lower,histogram.upper].map((cut,index)=>cut !== null && cut >= edges[0] && cut <= edges[edges.length-1] ? <line key={index} x1={x(cut)} x2={x(cut)} y1="7" y2="94" stroke="#a74e28" strokeDasharray="3 3"/> : null)}
      <line x1="12" y1="94" x2="288" y2="94" stroke="#b8b7b1"/>
      <text x="12" y="110">{format(edges[0])}</text><text x="288" y="110" textAnchor="end">{format(edges[edges.length-1])}</text>
      <text x="12" y="8">{peak.toLocaleString()} {cells}</text>
    </svg><small>{histogram.log ? logLabel : '%'}</small>
  </article>;
}
export function ExpressionQC({report,locale,active,canAnalyze,timeout}:{report:H5adReport;locale:Locale;active:boolean;canAnalyze:boolean;timeout:number}) {
  const t = qcText[locale], qc = report.qc;
  const [options,setOptions] = useState<QcOptions>(qc?.options ?? defaults);
  const [pending,setPending] = useState(false), [error,setError] = useState('');
  const donor = qc?.donor;
  const fields = ['minGenes','maxGenes','minCounts','maxCounts','maxMitoPct'] as const;
  const valid = qc && qc.status !== 'invalid_counts';
  const geneColumns = options.matrix === 'raw' ? report.rawGeneColumns ?? [] : report.geneColumns ?? [];
  return <section className="expression-qc" aria-labelledby="qc-title">
    <header className="qc-heading"><div><SlidersHorizontal size={17}/><h3 id="qc-title">{t.title}</h3><Info text={t.help}/></div><small>{qc ? `${t.saved} · ${qc.options.matrix} · ${qc.features.toLocaleString()} ${t.features}` : t.noQc}</small></header>
    <form onSubmit={event=>{event.preventDefault();setPending(true);setError('');void analyzeH5ad(options,report.sha256,timeout).catch(error=>setError(String(error))).finally(()=>setPending(false));}}>
      <div className="qc-mapping"><label><Users size={15}/>{t.donor}<select aria-label={t.donor} value={options.donorColumn} disabled={active || pending} onChange={e=>setOptions({...options,donorColumn:e.target.value})}><option value="">{t.unmapped}</option>{report.obsColumns.map(column=><option key={column}>{column}</option>)}</select></label><Info text={t.donorHelp}/><button type="submit" className="button" disabled={!canAnalyze || active || pending}><Play size={13}/>{t.run}</button></div>
      <details className="qc-options"><summary>{t.options}</summary><fieldset disabled={active || pending}><div className="qc-option-grid">
        <label>{t.matrix}<select value={options.matrix} onChange={e=>setOptions({...options,matrix:e.target.value,geneColumn:'_index'})}>{(report.matrices ?? ['X']).map(matrix=><option key={matrix}>{matrix}</option>)}</select></label>
        <label>{t.geneColumn}<select value={options.geneColumn} onChange={e=>setOptions({...options,geneColumn:e.target.value})}><option value="_index">{t.index}</option>{geneColumns.map(column=><option key={column}>{column}</option>)}</select></label>
        <label>{t.prefix}<input required maxLength={200} value={options.mitoPrefix} onChange={e=>setOptions({...options,mitoPrefix:e.target.value})}/></label>
        {fields.map(key=><label key={key}>{t[key]}<input type="number" min="0" max={key==='maxMitoPct'?100:undefined} step={key.includes('Genes')?1:'any'} required={key==='minGenes'||key==='minCounts'} value={options[key]??''} onChange={e=>setOptions({...options,[key]:e.target.value===''?null:Number(e.target.value)})}/></label>)}
      </div></fieldset></details>
    </form>
    {!canAnalyze ? <p className="source-note">{t.sourceUnavailable}</p> : null}
    {error ? <p role="alert" className="form-error">{t[error as keyof typeof t] ?? t.invalid_qc_options}</p> : null}
    {qc ? <>
      {valid ? <QcExports key={report.selection?.id ?? 'none'} report={report} locale={locale} active={active} dirty={JSON.stringify(options)!==JSON.stringify(qc.options)} timeout={timeout}/> : null}
      {qc.status==='invalid_counts' ? <div className="qc-warning"><TriangleAlert size={16}/><div><strong>{t.invalid}</strong><p>{t.invalidHelp}</p><span>{t.nonfinite}: {qc.invalidValues.nonfinite} · {t.negative}: {qc.invalidValues.negative} · {t.fractional}: {qc.invalidValues.fractional}</span></div></div> : <>
        <div className="qc-summary"><div><strong>{qc.retained?.toLocaleString()}</strong><span>{t.retained}</span></div><div><strong>{qc.excluded?.toLocaleString()}</strong><span>{t.excluded}</span></div><div className="qc-retention-track" role="img" aria-label={`${t.retained}: ${qc.retained} / ${qc.cells}`}><span style={{width:`${(qc.retained??0)/qc.cells*100}%`}}/></div><b>{((qc.retained??0)/qc.cells*100).toFixed(2)}%</b></div>
        {qc.status==='partial' ? <p className="qc-partial"><TriangleAlert size={13}/>{t.partial}</p> : null}
        <div className="qc-chart-grid">{(['counts','genes','mito'] as const).map(key=>qc.metrics[key] ? <HistogramPlot key={key} histogram={qc.metrics[key]!} title={t[key]} median={t.median} logLabel={t.log} cells={t.cells}/> : <article key={key} className="qc-chart qc-unavailable"><h4>{t.mito}</h4><TriangleAlert size={20}/><span>{t.noMito}</span><code>{qc.options.mitoPrefix} · {qc.mitoFeatures}</code></article>)}</div>
        {qc.zeroCells && qc.mitoFeatures ? <small className="source-note">{t.mitoUndefined}</small> : null}
        <details className="qc-failures"><summary>{t.failures}</summary><div>{Object.entries(qc.failures).map(([key,value])=><span key={key}>{t[key as keyof typeof t] ?? key}<b>{value.toLocaleString()}</b></span>)}</div></details>
      </>}
      {donor?.column ? <section className="qc-donors"><header><h4><Users size={15}/>{donor.column}</h4><span>{donor.distinct} {t.donors} · {t.missing} {donor.missing}</span><Info text={t.donorHelp}/></header>
        {donor.missingRetained ? <p className="qc-partial">{t.missing} · {t.after} {donor.missingRetained}</p> : null}
        {donor.distinct<2 ? <p className="qc-partial">{t.singleDonor}</p> : null}
        <div className="qc-donor-legend"><span className="donor-retained"/>{valid?t.after:t.before}<span className="donor-excluded"/>{t.excluded}</div>
        {donor.groups.map((row,index)=>{const max=Math.max(...donor.groups.map(r=>r.cells),1);return <div className="qc-donor-row" key={index}><span>{row.label}</span><div className="qc-donor-bar" role="img" aria-label={`${row.label}: ${row.retained??'—'} / ${row.cells}`} style={{width:`${row.cells/max*100}%`}}><span style={{width:`${(row.retained??row.cells)/row.cells*100}%`}}/></div><span>{row.retained?.toLocaleString()??'—'} / {row.cells.toLocaleString()}</span></div>;})}
        {donor.otherCells ? <small>{t.other}: {donor.otherCells.toLocaleString()}</small> : null}<small className="source-note">{t.unverified}</small>
      </section> : null}
    </> : null}
  </section>;
}

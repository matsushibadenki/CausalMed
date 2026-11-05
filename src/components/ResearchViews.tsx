import { H5adImport } from './H5adImport';
import { ArrowUpRight, Database, FlaskConical, Play, ShieldQuestion } from 'lucide-react';
import { PageTitle,Empty } from './ui';
import { formatDate } from '../lib/i18n';
import type { HypothesisStatus,ResearchDocument } from '../lib/types';

type Props={document:ResearchDocument;t:(s:string)=>string};
export function Datasets({document,t}:Props) {
  return <><PageTitle title={t('datasetsTitle')} help={t('datasetsHelp')}/><H5adImport locale={document.workspace.locale}/><div className="source-list">{document.sources.map(source=>{
    const record=document.imports.find(i=>i.sourceIds.includes(source.id));
    return <article key={source.id} className="source-record"><div className="source-heading"><Database size={22} strokeWidth={1.5}/><div><h2>{source.synthetic ? t(source.title) : source.title}</h2><span>{source.id} · {source.version}</span></div><span className="neutral-label">{t(source.synthetic?'syntheticData':'sourceUnverified')}</span></div>
      <dl className="source-metadata"><div><dt>{t('license')}</dt><dd>{source.license}</dd></div><div><dt>{t('records')}</dt><dd>{document.edges.filter(e=>e.sourceId===source.id).length}</dd></div><div><dt>{t('locator')}</dt><dd>{source.locator.startsWith('https://') ? <a href={source.locator} target="_blank" rel="noopener noreferrer">{source.locator}<ArrowUpRight size={13}/></a>:<code>{source.locator}</code>}</dd></div>{record?<><div><dt>{t('importedAt')}</dt><dd>{formatDate(record.importedAt,document.workspace.locale)}</dd></div><div><dt>{t('checksum')}</dt><dd><code>{record.sha256}</code></dd></div></>:null}</dl>
    </article>;
  })}</div></>;
}
export function Hypotheses({document,t,onExplore,onStatus,busy}:Props & {onExplore:()=>void;onStatus:(id:string,status:HypothesisStatus)=>void;busy:boolean}) {
  return <><PageTitle title={t('hypothesisTitle')} help={t('hypothesisHelp')}/>{document.hypotheses.length ? <div className="hypothesis-list">{document.hypotheses.map(hypothesis=><article key={hypothesis.id}><div className="hypothesis-heading"><FlaskConical size={20}/><h2>{hypothesis.title}</h2><select aria-label={`${t('hypotheses')}: ${hypothesis.title}`} value={hypothesis.status} disabled={busy} onChange={e=>onStatus(hypothesis.id,e.target.value as HypothesisStatus)}>{(['draft','planned','archived'] as const).map(status=><option key={status} value={status}>{t(status)}</option>)}</select></div><div className="hypothesis-tags"><code>{hypothesis.edgeId}</code><span>{t('unvalidated')}</span><span><ShieldQuestion size={14}/>{t('safetyUnknown')}</span></div><dl><div><dt>{t('interventionPlan')}</dt><dd>{hypothesis.intervention}</dd></div><div><dt>{t('falsification')}</dt><dd>{hypothesis.falsification}</dd></div></dl><time>{formatDate(hypothesis.createdAt,document.workspace.locale)}</time></article>)}</div>:<Empty title={t('noHypotheses')} description={t('noHypothesesHint')}><button className="button" onClick={onExplore}>{t('goEvidence')}</button></Empty>}</>;
}
export function BenchmarkView({document,t,onRun,busy,native}:Props & {onRun:()=>void;busy:boolean;native:boolean}) {
  const report=document.benchmarks[0];
  return <><PageTitle title={t('benchmarkTitle')} help={t('benchmarkHelp')}><button className="button primary" disabled={!native||busy} title={!native?t('previewHelp'):undefined} onClick={onRun}><Play size={14}/>{t(busy?'busy':'runBenchmark')}</button></PageTitle>
    <div className="benchmark-intro"><h2>M0 <span>{t('baseline')}</span></h2><dl className="metadata"><div><dt>{t('split')}</dt><dd>{t('splitValue')}</dd></div><div><dt>{t('heldOut')}</dt><dd>GENE-E</dd></div></dl><p className="source-note">{t('baselineOnly')}</p></div>
    {report ? <section className="benchmark-result"><div className="section-heading"><h2>{t(report.abstained?'abstained':'benchmarkCompleted')}</h2><time>{formatDate(report.createdAt,document.workspace.locale)}</time></div>{report.abstained?<p>{t(report.reason??'abstained')}</p>:<dl className="benchmark-metrics"><div><dt>{t('mae')}</dt><dd>{report.donorMae.toFixed(3)}<small>{t('arbitraryUnits')}</small></dd></div><div><dt>{t('controlMean')}</dt><dd>{report.controlMean.toFixed(3)}</dd></div><div><dt>{t('trainDonors')}</dt><dd>{report.trainDonors}</dd></div><div><dt>{t('testDonors')}</dt><dd>{report.testDonors}</dd></div></dl>}
      <details><summary>{t('manifest')}</summary><dl className="source-metadata"><div><dt>{t('model')}</dt><dd>{report.model}</dd></div><div><dt>SHA-256</dt><dd><code>{report.datasetSha256}</code></dd></div><div><dt>{t('trainSamples')}</dt><dd><code>{report.trainIds.join(', ')}</code></dd></div><div><dt>{t('testSamples')}</dt><dd><code>{report.testIds.join(', ')}</code></dd></div></dl></details>
    </section>:<Empty title={t('benchmarkEmpty')} description={t('benchmarkHint')}/>}</>;
}

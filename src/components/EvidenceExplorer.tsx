import { useMemo, useState } from 'react';
import { ArrowRight, ChevronRight, Search, Network } from 'lucide-react';
import { EvidenceLabel, Empty, IconButton, PageTitle } from './ui';
import type { EvidenceKind, ResearchDocument } from '../lib/types';
import { formatDate } from '../lib/i18n';

export function EvidenceExplorer({document,t,onSelect,onGraph,native}:{document:ResearchDocument;t:(s:string)=>string;onSelect:(id:string)=>void;onGraph:()=>void;native:boolean}) {
  const [query,setQuery]=useState('');
  const [filter,setFilter]=useState<EvidenceKind|'all'>('all');
  const rows=useMemo(()=>document.edges.filter(e=>(filter==='all'||e.evidence===filter)&&[e.source,e.target,e.id,t(e.relation)].join(' ').toLowerCase().includes(query.toLowerCase().trim())),[document.edges,query,filter,t]);
  const kinds: (EvidenceKind|'all')[]=['all','observation','intervention','hypothesis'];
  const activityLabels:Record<string,string>={import:'imported',hypothesis:'hypothesisCreated',status:'statusUpdated',benchmark:'benchmarkCompleted'};
  return <>
    <PageTitle title={t('evidenceTitle')} help={t('evidenceHelp')}>
      <IconButton label={native?t('graphHelp'):t('previewHelp')} disabled={!native} onClick={onGraph}><Network size={19}/></IconButton>
    </PageTitle>
    <div className="evidence-toolbar">
      <label className="search-field"><Search size={17}/><span className="sr-only">{t('searchLabel')}</span><input value={query} onChange={e=>setQuery(e.target.value)} placeholder={t('search')}/><kbd>⌕</kbd></label>
      <div className="filters" aria-label={t('kind')}>{kinds.map(kind=><button key={kind} aria-pressed={filter===kind} onClick={()=>setFilter(kind)}>{t(kind)}</button>)}</div>
    </div>
    {rows.length ? <div className="table-scroll"><table className="evidence-table"><thead><tr><th>{t('source')}</th><th>{t('relation')}</th><th>{t('target')}</th><th>{t('kind')}</th><th><span className="sr-only">{t('inspector')}</span></th></tr></thead>
      <tbody>{rows.map(edge=><tr key={edge.id} className={document.workspace.selectedEdgeId===edge.id?'selected':''} onClick={()=>onSelect(edge.id)}>
        <td><button className="molecule-button" aria-pressed={document.workspace.selectedEdgeId===edge.id} onClick={e=>{e.stopPropagation();onSelect(edge.id);}}>{edge.source}</button><span className="row-id">{edge.id}</span></td>
        <td><span className="relation-cell">{edge.relation==='associated'?<span className="association-mark">~</span>:<ArrowRight size={13}/>} {t(edge.relation)}</span></td><td className="molecule">{edge.target}</td><td><EvidenceLabel kind={edge.evidence} t={t}/></td><td><ChevronRight size={15}/></td>
      </tr>)}</tbody></table></div> : <Empty title={t('noResults')}><button className="button" onClick={()=>{setQuery('');setFilter('all');}}>{t('clearFilters')}</button></Empty>}
    <section className="research-notes"><h2>{t('notes')}</h2><p>{t('fixtureNote')}</p></section>
    <section className="activity"><h2>{t('recentActivity')}</h2>{document.activity.length ? <ul>{document.activity.slice(0,4).map((item,i)=><li key={`${item.at}-${i}`}><time dateTime={item.at}>{formatDate(item.at,document.workspace.locale)}</time><span>{t(activityLabels[item.kind]??item.kind)}</span></li>)}</ul>:<p className="muted">{t('noActivity')}</p>}</section>
  </>;
}

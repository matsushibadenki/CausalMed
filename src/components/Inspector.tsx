import { ArrowRight, PanelRight, PanelBottom, ExternalLink, GripVertical, AlertCircle, FlaskConical } from 'lucide-react';
import { EvidenceLabel, IconButton, Info } from './ui';
import type { Dock, Edge, ResearchDocument } from '../lib/types';

export function Inspector({document,t,onDock,onSelect,onCreate,native,onDragChange}:{document:ResearchDocument;t:(s:string)=>string;onDock:(dock:Dock)=>void;onSelect:(id:string)=>void;onCreate:(edge:Edge)=>void;native:boolean;onDragChange?:(value:boolean)=>void}) {
  const edge=document.edges.find(e=>e.id===document.workspace.selectedEdgeId);
  const source=document.sources.find(s=>s.id===edge?.sourceId);
  const content=(value:string)=>source?.synthetic?t(value):value;
  const conflicts=document.edges.filter(e=>edge?.contradictions.includes(e.id));
  return <aside className="inspector" aria-label={t('inspector')}>
    <header className="panel-header" draggable={native} onDragStart={event=>{event.dataTransfer.setData('text/causalmed-panel','inspector');onDragChange?.(true);}} onDragEnd={()=>onDragChange?.(false)} title={t('panelHelp')}>
      <h2><GripVertical size={14}/>{t('inspector')}</h2>
      <div className="panel-controls">
        <IconButton label={t('dockRight')} disabled={!native} onClick={()=>onDock('right')}><PanelRight size={15}/></IconButton>
        <IconButton label={t('dockBottom')} disabled={!native} onClick={()=>onDock('bottom')}><PanelBottom size={15}/></IconButton>
        <IconButton label={t('floatPanel')} disabled={!native} onClick={()=>onDock('floating')}><ExternalLink size={15}/></IconButton>
      </div>
    </header>
    {edge && source ? <>
      <div className="inspector-content">
        <section className="relationship-detail"><div className="detail-id">{edge.id}</div><h3><span>{edge.source}</span><ArrowRight size={19}/><span>{edge.target}</span></h3><EvidenceLabel kind={edge.evidence} t={t}/>
          <dl className="metadata"><div><dt>{t('relation')}</dt><dd>{t(edge.relation)}</dd></div><div><dt>{t('cellModel')}</dt><dd>{content(edge.cellType)}</dd></div><div><dt>{t('context')}</dt><dd>{content(edge.context)}</dd></div><div><dt>{t('version')}</dt><dd>{source.version}</dd></div></dl>
        </section>
        <section><h3>{t('provenance')}</h3><p className="source-id">{source.id}</p><p>{content(source.title)}</p><span className="source-note">{t(source.synthetic?'syntheticNotice':'importedNotice')}</span><blockquote>{content(edge.excerpt)}</blockquote></section>
        <section><div className="section-heading"><h3>{t('contradictions')}</h3><Info text={t('conflictHelp')}/></div>{conflicts.length ? conflicts.map(conflict=><button className="conflict-row" key={conflict.id} onClick={()=>onSelect(conflict.id)}><AlertCircle size={16}/><span><strong>{conflict.id}</strong><small>{conflict.source} · {t(conflict.relation)} · {conflict.target}</small></span><ArrowRight size={15}/></button>):<p className="muted">{t('noContradictions')}</p>}</section>
      </div>
      <footer className="inspector-footer"><button className="button primary" disabled={!native} title={!native?t('previewHelp'):undefined} onClick={()=>onCreate(edge)}><FlaskConical size={16}/>{t('createHypothesis')}</button></footer>
    </>:<p>{t('blankSelection')}</p>}
  </aside>;
}

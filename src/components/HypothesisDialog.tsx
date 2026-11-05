import { useEffect,useRef,useState } from 'react';
import { X } from 'lucide-react';
import { IconButton } from './ui';
import { createHypothesis } from '../lib/bridge';
import { errorMessage } from '../lib/i18n';
import type { Edge, Locale } from '../lib/types';

export function HypothesisDialog({edge,locale,t,onClose}:{edge:Edge;locale:Locale;t:(s:string)=>string;onClose:()=>void}) {
  const ref=useRef<HTMLDialogElement>(null);
  const [busy,setBusy]=useState(false);const [error,setError]=useState('');
  useEffect(()=>{ref.current?.showModal();ref.current?.querySelector('input')?.focus();},[]);
  return <dialog ref={ref} onCancel={event=>{if(busy)event.preventDefault();else onClose();}} onClick={event=>{if(event.target===ref.current&&!busy)onClose();}}>
    <form onSubmit={async event=>{event.preventDefault();const fields=new FormData(event.currentTarget);setBusy(true);setError('');try{await createHypothesis({edgeId:edge.id,title:String(fields.get('title')),intervention:String(fields.get('intervention')),falsification:String(fields.get('falsification'))});onClose();}catch(error){setError(errorMessage(error,locale));}finally{setBusy(false);}}}>
      <div className="dialog-heading"><h2>{t('newHypothesis')}</h2><IconButton label={t('close')} onClick={onClose} disabled={busy}><X size={18}/></IconButton></div>
      <p className="dialog-context">{edge.source} {edge.relation === 'activates' ? '→' : edge.relation === 'inhibits' ? '⊣' : '~'} {edge.target}<span>{edge.id} · {t(edge.relation)} · {t('unvalidated')}</span></p>
      <label>{t('title')}<input name="title" required maxLength={200} autoFocus placeholder={t('titlePlaceholder')}/></label>
      <label>{t('interventionPlan')}<textarea name="intervention" required maxLength={2000} placeholder={t('interventionPlaceholder')}/></label>
      <label>{t('falsification')}<textarea name="falsification" required maxLength={4000} placeholder={t('falsificationPlaceholder')}/></label>
      {error?<p className="form-error" role="alert">{error}</p>:null}
      <footer><button type="button" className="button" disabled={busy} onClick={onClose}>{t('cancel')}</button><button className="button primary" disabled={busy} type="submit">{t(busy?'busy':'saveDraft')}</button></footer>
    </form>
  </dialog>;
}

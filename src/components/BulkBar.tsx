import { useEffect,useRef } from 'react';
import { Trash2,Undo2,X } from 'lucide-react';
import { bulkText } from '../lib/bulkI18n';
import { selectionLimit,type BulkSelection } from '../lib/bulkSelection';
import type { Locale } from '../lib/types';
import { IconButton,Info } from './ui';
export function BulkBar({locale,selection,visible,eligible,trash,disabled,onApply}:{locale:Locale;selection:BulkSelection;visible:string[];eligible:string[];trash:boolean;disabled:boolean;onApply:()=>void}){
 const t=bulkText[locale],ref=useRef<HTMLInputElement>(null);
 const count=eligible.filter(id=>selection.ids.has(id)).length,all=eligible.length>0&&count===eligible.length;
 useEffect(()=>{if(ref.current)ref.current.indeterminate=count>0&&!all;},[count,all]);
 const shown=new Set(visible),hidden=[...selection.ids].filter(id=>!shown.has(id)).length;
 return <div className="bulk-bar"><label><input ref={ref} type="checkbox" aria-label={t.selectPage} checked={all} disabled={disabled||!eligible.length||(!all&&selection.ids.size+eligible.length-count>selectionLimit)} onChange={()=>selection.page(eligible)}/>{t.selectPage}</label><span aria-live="polite">{selection.ids.size.toLocaleString()} {t.selected}{hidden?` · ${hidden.toLocaleString()} ${t.hidden}`:''}</span><Info text={t.help}/><IconButton label={t.clear} disabled={disabled||!selection.ids.size} onClick={selection.clear}><X size={14}/></IconButton><button type="button" className="button" disabled={disabled||!selection.ids.size} onClick={onApply}>{trash?<Undo2 size={14}/>:<Trash2 size={14}/>} {trash?t.restore:t.move}</button></div>;
}

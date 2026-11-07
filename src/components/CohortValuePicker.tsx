import { useState } from 'react';
import { CheckSquare, Square } from 'lucide-react';
import type { Locale } from '../lib/types';
import { definitionText } from '../lib/definitionI18n';
import { IconButton } from './ui';
export function CohortValuePicker({label,column,rows,values,onChange,disabled,locale}:{label:string;column:string;rows:{label:string;count:number}[];values:string[];onChange:(values:string[])=>void;disabled:boolean;locale:Locale}) {
 const t=definitionText[locale], [query,setQuery]=useState('');
 const shown=rows.filter(r=>r.label.toLocaleLowerCase().includes(query.toLocaleLowerCase()));
 function toggle(value:string){onChange(values.includes(value)?values.filter(v=>v!==value):[...values,value].sort());}
 return <fieldset className="cohort-picker" disabled={disabled}><legend>{label} · <code>{column||'—'}</code> <b>{values.length}/{rows.length}</b></legend><div className="cohort-picker-tools">{rows.length>6?<input type="search" aria-label={`${label} · ${t.searchValues}`} placeholder={t.searchValues} value={query} onChange={e=>setQuery(e.target.value)}/>:null}<IconButton label={`${label} · ${t.selectAll}`} disabled={disabled||!rows.length} onClick={()=>onChange(rows.map(r=>r.label).sort())}><CheckSquare size={13}/></IconButton><IconButton label={`${label} · ${t.clearValues}`} disabled={disabled||!values.length} onClick={()=>onChange([])}><Square size={13}/></IconButton></div><div className="cohort-choice-list">{shown.map(r=><label key={r.label}><input type="checkbox" checked={values.includes(r.label)} onChange={()=>toggle(r.label)}/><span>{r.label}</span><small>{r.count.toLocaleString(locale)}</small></label>)}</div></fieldset>;
}

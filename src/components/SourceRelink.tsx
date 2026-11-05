import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Link, Link2Off, FolderSearch } from 'lucide-react';
import { useH5ad, refreshH5ad, workerActive } from '../lib/h5ad';
import type { Locale } from '../lib/types';
import { projectText } from '../lib/projectI18n';
import { Info } from './ui';
export function SourceRelink({locale,timeout}:{locale:Locale;timeout:number}) {
 const job=useH5ad(),t=projectText[locale];const [pending,setPending]=useState(false),[error,setError]=useState('');
 if(!job.report?.selection)return null;
 async function run(){setPending(true);setError('');try{await invoke('relink_h5ad',{selectionId:job.report!.selection!.id,timeoutSeconds:timeout});await refreshH5ad();}catch(e){setError(String(e));}finally{setPending(false);}}
 return <div className="source-relink"><span>{job.sourceLinked?<Link size={14}/>:<Link2Off size={14}/>} {job.sourceLinked?t.linked:t.missing}{job.sourceName?<code>{job.sourceName}</code>:null}</span><button type="button" className="button" disabled={pending||workerActive(job.status)} onClick={()=>void run()}><FolderSearch size={14}/>{t.relink}</button><Info text={t.sourceHelp}/>{error?<p role="alert">{t[error as keyof typeof t]??t.storage_error}</p>:null}</div>;
}

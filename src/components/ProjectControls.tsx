import { ProjectBrowser } from './ProjectBrowser';
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Save, FolderOpen, Folders, LoaderCircle } from 'lucide-react';
import { native, refresh } from '../lib/bridge';
import { refreshH5ad, useH5ad, workerActive } from '../lib/h5ad';
import { projectText } from '../lib/projectI18n';
import type { Locale } from '../lib/types';
import { IconButton, Info } from './ui';
export function ProjectControls({locale,onRestore}:{locale:Locale;onRestore:()=>void}) {
 const t=projectText[locale],job=useH5ad();
 const [browser,setBrowser]=useState(false);
 const [pending,setPending]=useState(''),[message,setMessage]=useState(''),[error,setError]=useState(false);
 async function run(kind:'save'|'open') {
  setPending(kind);setMessage('');setError(false);
  try {
   const done=await invoke<boolean>(kind==='save'?'save_project':'restore_project');
   if(done){await Promise.all([refresh(),refreshH5ad(true)]);setMessage(kind==='save'?t.saved:t.restored);if(kind==='open')onRestore();}
  }catch(error){setError(true);setMessage(t[String(error) as keyof typeof t]??t.storage_error);}
  finally{setPending('');}
 }
 return <div className="project-controls"><IconButton label={t.projects} disabled={!native||!!pending} onClick={()=>setBrowser(true)}><Folders size={18}/></IconButton><IconButton label={t.save} disabled={!native||!!pending||workerActive(job.status)} onClick={()=>void run('save')}><Save size={18}/></IconButton><IconButton label={t.open} disabled={!native||!!pending||workerActive(job.status)} onClick={()=>void run('open')}><FolderOpen size={18}/></IconButton><Info text={t.help}/>
 {browser?<ProjectBrowser locale={locale} onClose={()=>setBrowser(false)} onSwitch={onRestore}/>:null}
 {pending||message?<div className={`project-feedback ${error?'form-error':''}`} role={error?'alert':'status'}>{pending?<><LoaderCircle size={13} className="spin"/>{pending==='save'?t.saving:t.restoring}</>:message}</div>:null}</div>;
}

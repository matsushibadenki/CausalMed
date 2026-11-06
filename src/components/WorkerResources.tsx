import { useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { Cpu, Save, RotateCcw } from 'lucide-react';
import { refreshH5ad, useH5ad, workerActive } from '../lib/h5ad';
import type { ResourceLimits } from '../lib/h5ad';
import { resourceText } from '../lib/resourceI18n';
import { datasetText } from '../lib/datasetI18n';
import { Info, IconButton } from './ui';
import type { Locale } from '../lib/types';
export function WorkerResources({locale}:{locale:Locale}) {
 const job=useH5ad();
 return <ResourceControls key={job.projectKey} locale={locale}/>;
}
function ResourceControls({locale}:{locale:Locale}) {
 const job=useH5ad(), t=resourceText[locale], dt=datasetText[locale], settings=job.resourceSettings;
 const [edit,setEdit]=useState<{limits:ResourceLimits;revision:number}|null>(null),[pending,setPending]=useState(false),[error,setError]=useState(''),[saved,setSaved]=useState(false);
 const active=workerActive(job.status), disabled=active||pending||!isTauri(), limits=edit?.limits??settings.limits;
 const usage=job.resources, execution=job.executionLimits??settings.limits;
 function change(key:keyof ResourceLimits,value:number){setEdit({limits:{...limits,[key]:value},revision:edit?.revision??settings.revision});setSaved(false);}
 function message(code:string){return t[code as keyof typeof t]??dt[code as keyof typeof dt]??t.error;}
 async function save(){setPending(true);setError('');try{await invoke('set_worker_limits',{projectKey:job.projectKey,limits,expectedRevision:edit?.revision??settings.revision});await refreshH5ad(true);setEdit(null);setSaved(true);}catch(e){setError(String(e));}finally{setPending(false);}}
 const cpu=(usage.cpuMillis/1000).toFixed(1), peak=(usage.peakResidentBytes/1048576).toFixed(1), rss=(usage.residentBytes/1048576).toFixed(1);
 return <section className="worker-resources"><header><Cpu size={16}/><h2>{t.title}</h2><Info text={t.help}/><code className="worker-runtime">Python {job.runtime.pythonVersion??'—'} · {message(`runtime_${job.runtime.kind}`)}</code><span>{active?t.live:t.last}</span></header>
 <div className="resource-metrics"><div><span>{t.rss}</span><strong>{usage.samples?`${rss} MiB`:'—'}</strong></div><div><span>{t.peak}</span><strong>{usage.samples?`${peak} / ${execution.memoryMib} MiB`:'—'}</strong><meter aria-label={t.peak} min={0} max={execution.memoryMib} low={execution.memoryMib*0.7} high={execution.memoryMib*0.9} optimum={0} value={usage.peakResidentBytes/1048576}/></div><div><span>{t.cpuUsed}</span><strong>{usage.samples?`${cpu} / ${execution.cpuSeconds} s`:'—'}</strong><meter aria-label={t.cpuUsed} min={0} max={execution.cpuSeconds} low={execution.cpuSeconds*0.7} high={execution.cpuSeconds*0.9} optimum={0} value={usage.cpuMillis/1000}/></div></div>
 {!usage.samples?<small>{t.idle}</small>:null}
 {job.runtime.error?<p role="alert">{message(job.runtime.error)}</p>:null}
 {job.error&&job.error in t?<p role="alert">{message(job.error)}</p>:null}{!job.resourceMonitorSupported?<p role="alert">{t.resource_monitor_unsupported}</p>:null}{job.resourceSettingsInvalid?<p role="alert">{t.resource_settings_invalid}</p>:null}
 <details><summary>{t.settings} · {settings.limits.memoryMib} MiB · {settings.limits.cpuSeconds} s · ×{settings.limits.threads}</summary><form onSubmit={e=>{e.preventDefault();void save();}}><label>{t.memory}<input type="number" min={64} max={65536} step={1} aria-label={t.memory} value={limits.memoryMib} disabled={disabled} onChange={e=>change('memoryMib',Number(e.target.value))}/></label><label>{t.cpu}<input type="number" min={1} max={3600} step={1} aria-label={t.cpu} value={limits.cpuSeconds} disabled={disabled} onChange={e=>change('cpuSeconds',Number(e.target.value))}/></label><label>{t.threads}<select aria-label={t.threads} value={limits.threads} disabled={disabled} onChange={e=>change('threads',Number(e.target.value))}>{[1,2,3,4,5,6,7,8].map(n=><option key={n}>{n}</option>)}</select></label><button className="button" disabled={disabled}><Save size={14}/>{t.save}</button><IconButton label={t.reload} disabled={disabled} onClick={()=>{setEdit(null);setError('');setSaved(false);void refreshH5ad(true);}}><RotateCcw size={14}/></IconButton></form></details>
 {error?<p role="alert">{message(error)}</p>:null}{saved?<small role="status">{t.saved}</small>:null}
 </section>;
}

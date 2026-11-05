import { useEffect, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { QcOptions } from './h5ad';
export type ListQuery={search:string;filter:string;sort:string;page:number;pageSize:number;trash:boolean};
export type ListPage<T>={scope:string;items:T[];total:number;unfilteredTotal:number;page:number;pageSize:number;pages:number};
export type HistoryRow={id:string;name:string;createdAt:string;sourceSha256:string;cells:number;retained:number;excluded:number;status:string;options:QcOptions};
export type ProjectRow={id:string;name:string;active:boolean;createdAt:number|null;historyCount:number;selectedId:string|null;retained:number|null;excluded:number|null;donorColumn:string|null;sourceName:string|null;sourceLinked:boolean;trashed:boolean};
export function usePagedList<T>(command:'query_projects'|'query_qc_history',projectKey:string,revision:string){
 const [query,setQuery]=useState<ListQuery>({search:'',filter:'all',sort:command==='query_projects'?'active_first':'newest',page:1,pageSize:10,trash:false});
 const [result,setResult]=useState<{key:string;data:ListPage<T>}|null>(null),[error,setError]=useState(''),[loading,setLoading]=useState(true),[version,setVersion]=useState(0);
 const key=JSON.stringify([projectKey,query,revision,version]);
 useEffect(()=>{let current=true;setLoading(true);setError('');const timer=setTimeout(()=>{if(!isTauri()){setResult({key,data:{scope:projectKey,items:[],total:0,unfilteredTotal:0,page:1,pageSize:query.pageSize,pages:1}});setLoading(false);return;}void invoke<ListPage<T>>(command,{query,...(command==='query_qc_history'?{projectKey}:{})}).then(data=>{if(current&&data.scope===projectKey)setResult({key,data});}).catch(e=>{if(current)setError(String(e));}).finally(()=>{if(current)setLoading(false);});},180);return()=>{current=false;clearTimeout(timer);};},[command,projectKey,query,revision,version,key]);
 return {query,data:result?.key===key?result.data:null,error,loading,change:(patch:Partial<ListQuery>)=>setQuery(q=>({...q,...patch,page:patch.page??1})),reload:()=>setVersion(v=>v+1)};
}

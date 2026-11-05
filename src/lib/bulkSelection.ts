import { useEffect, useReducer } from 'react';
export const selectionLimit=10_000;
export type SelectionState={scope:string;ids:Set<string>};
type Action={type:'scope';scope:string}|{type:'clear';scope:string}|{type:'toggle'|'page'|'remove';scope:string;ids:string[]};
export const emptySelection=(scope:string):SelectionState=>({scope,ids:new Set()});
export function selectionReducer(state:SelectionState,action:Action):SelectionState{
 if(action.type==='scope')return state.scope===action.scope?state:emptySelection(action.scope);
 if(state.scope!==action.scope)return state; // Late receipts must not alter a new project/bucket.
 if(action.type==='clear')return emptySelection(state.scope);
 const ids=new Set(state.ids);
 if(action.type==='remove'){action.ids.forEach(id=>ids.delete(id));return {...state,ids};}
 const all=action.ids.length>0&&action.ids.every(id=>ids.has(id));
 for(const id of action.ids){if(all)ids.delete(id);else if(ids.size<selectionLimit)ids.add(id);}
 return {...state,ids};
}
export function useBulkSelection(scope:string){
 const [state,dispatch]=useReducer(selectionReducer,scope,emptySelection);
 useEffect(()=>{dispatch({type:'scope',scope});},[scope]);
 const ids=state.scope===scope?state.ids:new Set<string>();
 return {scope,ids,toggle:(id:string)=>dispatch({type:'toggle',scope,ids:[id]}),page:(ids:string[])=>dispatch({type:'page',scope,ids}),clear:()=>dispatch({type:'clear',scope}),remove:(owner:string,ids:string[])=>dispatch({type:'remove',scope:owner,ids})};
}
export type BulkSelection=ReturnType<typeof useBulkSelection>;
export type BatchResult={scope:string;succeeded:string[];failed:{id:string;error:string}[]};

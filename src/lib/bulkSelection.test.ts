import {expect,it} from 'vitest';
import {emptySelection,selectionReducer,selectionLimit} from './bulkSelection';
it('retains selected IDs across disjoint pages and removes only the visible selected page',()=>{
 let s=emptySelection('A:live');s=selectionReducer(s,{type:'page',scope:s.scope,ids:['a','b']});s=selectionReducer(s,{type:'page',scope:s.scope,ids:['c','d']});
 expect([...s.ids]).toEqual(['a','b','c','d']);
 // A filtered page contains a previously selected item plus a new match.
 s=selectionReducer(s,{type:'page',scope:s.scope,ids:['a','e']});expect([...s.ids]).toEqual(['a','b','c','d','e']);
 s=selectionReducer(s,{type:'page',scope:s.scope,ids:['a','e']});expect([...s.ids]).toEqual(['b','c','d']);
});
it('clears on project/bucket changes and ignores late action receipts',()=>{
 let s=selectionReducer(emptySelection('A:live'),{type:'toggle',scope:'A:live',ids:['a']});
 s=selectionReducer(s,{type:'scope',scope:'A:trash'});expect(s.ids.size).toBe(0);
 s=selectionReducer(s,{type:'toggle',scope:'A:trash',ids:['b']});
 s=selectionReducer(s,{type:'remove',scope:'A:live',ids:['b']});expect([...s.ids]).toEqual(['b']);
 s=selectionReducer(s,{type:'scope',scope:'B:trash'});expect(s.ids.size).toBe(0);
 s=selectionReducer(s,{type:'scope',scope:'A:live'});expect(s.ids.size).toBe(0);
});
it('keeps failed and unapplied targets selected after a partial receipt',()=>{
 let s=selectionReducer(emptySelection('A'),{type:'page',scope:'A',ids:['ok','failed','hidden']});
 s=selectionReducer(s,{type:'remove',scope:'A',ids:['ok']});expect([...s.ids]).toEqual(['failed','hidden']);
 s=selectionReducer(s,{type:'toggle',scope:'A',ids:['failed']});expect([...s.ids]).toEqual(['hidden']);
 s=selectionReducer(s,{type:'clear',scope:'A'});expect(s.ids.size).toBe(0);
});
it('bounds large selections without losing already-selected targets',()=>{
 const ids=Array.from({length:selectionLimit+50},(_,i)=>String(i));
 let s=selectionReducer(emptySelection('A'),{type:'page',scope:'A',ids});expect(s.ids.size).toBe(selectionLimit);
 s=selectionReducer(s,{type:'toggle',scope:'A',ids:['0']});expect(s.ids.size).toBe(selectionLimit-1);
 s=selectionReducer(s,{type:'toggle',scope:'A',ids:['new']});expect(s.ids.size).toBe(selectionLimit);
});

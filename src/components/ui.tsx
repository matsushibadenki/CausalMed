import { Info as InfoIcon, X, SearchX } from 'lucide-react';
import type { ButtonHTMLAttributes, ReactNode } from 'react';
import type { EvidenceKind } from '../lib/types';

export function IconButton({ label, children, ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { label:string; children:ReactNode }) {
  return <button type="button" className="icon-button" title={label} aria-label={label} {...props}>{children}</button>;
}
export function Info({ text }: {text:string}) {
  return <span className="info"><button className="icon-button" type="button" aria-label={text}><InfoIcon size={16}/></button><span role="tooltip">{text}</span></span>;
}
export function EvidenceLabel({ kind, t }: { kind:EvidenceKind; t:(key:string)=>string }) {
  return <span className={`evidence-label ${kind}`}><span className="evidence-dot"/>{t(kind)}</span>;
}
export function PageTitle({title,help,children}:{title:string;help:string;children?:ReactNode}) {
  return <header className="page-heading"><div><h1>{title}</h1><Info text={help}/></div>{children}</header>;
}
export function Empty({title,description,children}:{title:string;description?:string;children?:ReactNode}) {
  return <div className="empty"><SearchX size={25} strokeWidth={1.4}/><h2>{title}</h2>{description ? <p>{description}</p> : null}{children}</div>;
}
export function ErrorBanner({ message, dismiss, closeLabel }: {message:string;dismiss:()=>void;closeLabel:string}) {
  return <div className="error-banner" role="alert"><span>{message}</span><IconButton label={closeLabel} onClick={dismiss}><X size={16}/></IconButton></div>;
}

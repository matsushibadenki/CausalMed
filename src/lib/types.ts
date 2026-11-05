export type Locale = 'en' | 'ja' | 'zh';
export type EvidenceKind = 'observation' | 'intervention' | 'hypothesis';
export type Relation = 'activates' | 'inhibits' | 'associated';
export type Dock = 'right' | 'bottom' | 'floating';
export type HypothesisStatus = 'draft' | 'planned' | 'archived';
export interface Source { id: string; title: string; locator: string; license: string; version: string; synthetic: boolean }
export interface Edge { id: string; source: string; target: string; relation: Relation; evidence: EvidenceKind; sourceId: string; cellType: string; context: string; excerpt: string; contradictions: string[] }
export interface ImportRecord { id: string; sha256: string; importedAt: string; sourceIds: string[]; edgeCount: number }
export interface NewHypothesis { title: string; edgeId: string; intervention: string; falsification: string }
export interface Hypothesis extends NewHypothesis { id: string; status: HypothesisStatus; createdAt: string; safety: string }
export interface Benchmark { id: string; createdAt: string; model: string; synthetic: boolean; datasetSha256: string; heldOutDataset: string; heldOutPerturbation: string; trainIds: string[]; testIds: string[]; trainDonors: number; testDonors: number; controlMean: number; donorMae: number; abstained: boolean; reason: string | null }
export interface ResearchDocument {
  schemaVersion: number; revision: number; projectName: string;
  sources: Source[]; edges: Edge[]; imports: ImportRecord[];
  hypotheses: Hypothesis[]; benchmarks: Benchmark[];
  activity: { at: string; kind: string; reference: string }[];
  workspace: { locale: Locale; selectedEdgeId: string | null; inspectorDock: Dock };
}
export type View = 'evidence' | 'datasets' | 'hypotheses' | 'benchmark';

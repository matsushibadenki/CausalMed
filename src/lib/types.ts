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
  evaluationPlan:EvaluationPlan; evaluations:EvaluationRecord[];
  schemaVersion: number; revision: number; projectName: string;
  cohortDefinitions:CohortDefinition[]; cohorts:CohortReview[]; datasets: DatasetRecord[]; sources: Source[]; edges: Edge[]; imports: ImportRecord[];
  hypotheses: Hypothesis[]; benchmarks: Benchmark[];
  activity: { at: string; kind: string; reference: string }[];
  workspace: { locale: Locale; selectedEdgeId: string | null; inspectorDock: Dock };
}
export type View = 'evidence' | 'datasets' | 'hypotheses' | 'benchmark';

export interface AnnotationColumn { name:string; dtype:string; kind:string; nullable:boolean|null }
export interface DatasetMetadata { name:string; sourceUrl:string; sourceVersion:string; license:string; licenseUrl:string; citation:string; notes:string; donorColumn:string; conditionColumn:string; perturbationColumn:string; cellTypeColumn:string; reviewed:boolean }
export interface DatasetRecord { observation:{sha256:string;name:string;cells:number;genes:number;annotationColumnsTotal:number;annotationSchema:AnnotationColumn[];anndataVersion:string};metadata:DatasetMetadata;createdAt:string;updatedAt:string;reviewedAt:string|null }
export interface CohortObservation { sourceSha256:string;qcId:string;manifestSha256:string;metadataSha256:string;donorColumn:string;conditionColumn:string;perturbationColumn:string;retained:number;missingRetained:number;qcStatus:string;donors:{label:string;retained:number}[];conditionValue:string|null;perturbationValue:string|null }
export interface DonorMapping {label:string;canonicalId:string;reviewed:boolean;evidence:string}
export interface CohortSettings {studyId:string;contextId:string;perturbationId:string;partition:string;isControl:boolean;reviewed:boolean;evidence:string;mappings:DonorMapping[]}
export interface CohortReview {observation:CohortObservation;settings:CohortSettings;updatedAt:string}
export interface Eligibility {ready:boolean;documentRevision:number;checkedAt:string;issues:string[];cohorts:{sourceSha256:string;issues:string[];mapped:number;total:number;participating:boolean}[];trainDonors:number;testDonors:number;overlaps:string[]}
export interface CohortDefinition {payload:{id:string;name:string;sourceSha256:string;sourceCells:number;sourceGenes:number;metadata:DatasetMetadata;alignment:CohortReview;conditionValue:string;perturbationValue:string;conditionValues?:string[];perturbationValues?:string[];createdAt:string};sha256:string;exports:{sha256:string;fileName:string;bytes:number;retained:number;matched:number}[]}

export interface OutcomeSpec {featureId:string;featureColumn:string}
export interface EvaluationPlan {spec:OutcomeSpec;measurementEvidence:string;reviewedFor:string|null}
export interface EvaluationContext {version:number;spec:OutcomeSpec;inputs:{sourceSha256:string;sourceName:string;metadata:DatasetMetadata;cohort:CohortReview}[]}
export interface SourceOutcome {sourceSha256:string;qcId:string;matrix:string;featureCount:number;featureSpaceSha256:string|null;matchedFeatures:number;retained:number;missingDonorCells:number;issues:string[];donors:{label:string;cells:number;mean:number}[]}
export interface EvaluationRecord {id:string;createdAt:string;kind:'inspection'|'baseline';context:EvaluationContext;contextSha256:string;inspectionId:string|null;outcome:{version:number;contextSha256:string;inputs:SourceOutcome[]}|null;eligibility:Eligibility|null;measurementEvidence:string;reviewedFor:string|null;abstained:boolean;reasons:{code:string;sourceSha256:string|null}[];controlMean:number|null;mae:number|null;trainDonors:number;testDonors:number}

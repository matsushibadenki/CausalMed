import type { Locale } from './types';

const en = {
  evidence: 'Evidence', datasets: 'Datasets', hypotheses: 'Hypotheses', benchmark: 'Benchmark',
  research: 'IPF research', local: 'Local workspace', preview: 'Read-only preview',
  previewHelp: 'Open the desktop app to import, save, run benchmarks and detach panels. This browser preview does not store research data.',
  synthetic: 'Synthetic demo', mixed: 'Mixed sources', researchOnly: 'Research use only',
  evidenceTitle: 'Evidence explorer', evidenceHelp: 'Observation, intervention evidence and unverified hypotheses remain distinct. Source labels are supplied by the importer, not independently verified.',
  search: 'Find a molecule or relation', searchLabel: 'Search relationships', all: 'All',
  observation: 'Observation', intervention: 'Intervention', hypothesis: 'Hypothesis',
  source: 'Source', relation: 'Relation', target: 'Target', kind: 'Evidence',
  activates: 'Activates', inhibits: 'Inhibits', associated: 'Associated',
  inspector: 'Evidence inspector', cellModel: 'Cell model', context: 'Context', version: 'Version',
  provenance: 'Provenance', contradictions: 'Contradictions', noContradictions: 'No conflicting records registered',
  conflictHelp: 'Conflicts are explicitly linked records, not an automatic judgment of validity. Compare conditions and source excerpts.',
  synthetic_source: 'CausalMed synthetic fixture', synthetic_fibroblast: 'Synthetic fibroblast', in_vitro_fixture: 'In vitro fixture',
  synthetic_excerpt: 'Hand-authored test relationship. It is not a biological observation or a validated causal effect.',
  syntheticNotice: 'Synthetic fixture · not biological evidence', importedNotice: 'Imported record · source not independently verified',
  notes: 'Research notes', fixtureNote: 'This workspace includes synthetic test relationships. No treatment targets have been validated.',
  recentActivity: 'Recent activity', noActivity: 'No saved activity in this preview.',
  import: 'Import evidence', importHelp: 'Import a CausalMed evidence bundle (.json), up to 2 MiB. Sources, excerpts and context are required.',
  export: 'Export workspace', exportHelp: 'Save a JSON snapshot with hypotheses, source provenance and benchmark manifests.',
  imported: 'Evidence imported', hypothesisCreated: 'Hypothesis created', statusUpdated: 'Hypothesis updated', benchmarkCompleted: 'Benchmark completed',
  createHypothesis: 'Create hypothesis', hypothesisTitle: 'Hypothesis studio', hypothesisHelp: 'Draft a falsifiable research question from a specific evidence record. Drafts do not promote a relationship to established causality.',
  newHypothesis: 'New hypothesis', title: 'Title', interventionPlan: 'Intervention plan', falsification: 'Falsification criterion',
  titlePlaceholder: 'A testable research question', interventionPlaceholder: 'Target, manipulation, cell condition and controls', falsificationPlaceholder: 'Which independent experimental result would refute this hypothesis?',
  saveDraft: 'Save draft', cancel: 'Cancel', close: 'Close', draft: 'Draft', planned: 'Planned', archived: 'Archived',
  safetyUnknown: 'Safety unknown', unvalidated: 'Unvalidated', noHypotheses: 'No hypotheses yet', noHypothesesHint: 'Select a relationship in Evidence to draft a testable hypothesis.',
  goEvidence: 'Explore evidence', noResults: 'No matching relationships', clearFilters: 'Clear filters',
  dockRight: 'Dock right', dockBottom: 'Dock below', floatPanel: 'Detach inspector', panelHelp: 'Drag this header to a docking area, or use the docking controls.',
  panelDetached: 'Inspector is in a separate window', restorePanel: 'Dock inspector',
  graph: 'Open GPU relationship preview', graphHelp: 'Native GPU preview of the selected relation. Source is on the left; target on the right. Solid: intervention; dashed: observation or hypothesis. Full graph exploration is planned.',
  datasetsTitle: 'Source library', datasetsHelp: 'Inspect H5AD dimensions and annotations, or evidence-bundle provenance. Expression QC previews are available; evaluation mapping is still pending.',
  license: 'License', locator: 'Source locator', checksum: 'Import SHA-256', importedAt: 'Imported at', records: 'relationships', sources: 'sources',
  sourceUnverified: 'Unverified source', syntheticData: 'Synthetic data',
  benchmarkTitle: 'Model benchmark', benchmarkHelp: 'M0 predicts the training control mean. Cell measurements are averaged per donor; donors have equal weight. This synthetic smoke test does not evaluate medical prediction quality.',
  runBenchmark: 'Run synthetic M0', benchmarkEmpty: 'No benchmark runs yet', benchmarkHint: 'Run the bundled synthetic dataset to check the evaluation pipeline.',
  baseline: 'Control mean baseline', split: 'Evaluation split', splitValue: 'Held-out dataset + perturbation + donors',
  model: 'Model', mae: 'Donor MAE', controlMean: 'Training control mean', trainDonors: 'Training control donors', testDonors: 'Test donors',
  arbitraryUnits: 'arbitrary units', heldOut: 'Held-out perturbation', manifest: 'Split manifest', trainSamples: 'Training sample IDs', testSamples: 'Test sample IDs',
  abstained: 'Prediction withheld', insufficient_independent_donors: 'At least two independent control and test donors are required.', single_condition_required: 'Exactly one matched cell condition is required.',
  baselineOnly: 'M0 only · no model superiority established', loading: 'Opening workspace…', busy: 'Working…', retry: 'Retry',
  operationFailed: 'The operation failed. Your last saved workspace is retained.',
  desktop_required: 'This action is available in the desktop app.',
  import_too_large: 'The file exceeds 2 MiB. Import a smaller evidence bundle.', already_imported: 'This exact evidence bundle has already been imported.',
  duplicate_source: 'A source ID already exists. Use a new source version with a unique ID.', duplicate_edge: 'A relationship ID already exists. Use unique IDs.',
  missing_source: 'A relationship references a missing source.', invalid_source: 'Source metadata is incomplete or too long.', invalid_source_locator: 'Use an HTTPS source URL, or a fixture URI for synthetic data.',
  invalid_edge: 'A relationship is missing a molecule, context or excerpt.', invalid_contradiction: 'A contradiction must reference another record for the same source and target molecules.',
  observation_cannot_assert_causality: 'Observations must use the “associated” relation. They cannot assert activation or inhibition.',
  unsupported_schema: 'Unsupported schema version. Use schemaVersion 1.', invalid_bundle_size: 'The bundle is empty or exceeds the first-release workspace limit (10,000 relationships).',
  invalid_json: 'The JSON does not match the evidence bundle format. Check the example in fixtures/.',
  hypothesis_fields_required: 'Complete the title, intervention plan and falsification criterion.',
  edge_not_found: 'This relationship no longer exists. Select another record.', hypothesis_not_found: 'This hypothesis no longer exists.',
  copy: 'Copy', copied: 'Copied', blankSelection: 'Select a relationship', revision: 'Revision',
} as const;
type Messages = { [K in keyof typeof en]: string };
const ja: Messages = {
  evidence:'証拠',datasets:'データセット',hypotheses:'仮説',benchmark:'ベンチマーク',research:'IPF 研究',local:'ローカルワークスペース',preview:'閲覧プレビュー',
  previewHelp:'取り込み・保存・評価実行・パネル分離はデスクトップアプリで利用できます。このプレビューは研究データを保存しません。',
  synthetic:'合成データ',mixed:'混合ソース',researchOnly:'研究用',evidenceTitle:'証拠エクスプローラー',evidenceHelp:'観測・介入実験・未検証の仮説を区別します。証拠種別は取り込み時の申告であり、出典の独立検証は行っていません。',
  search:'分子・関係を検索',searchLabel:'関係を検索',all:'すべて',observation:'観測',intervention:'介入実験',hypothesis:'仮説',source:'起点',relation:'関係',target:'対象',kind:'証拠',activates:'活性化',inhibits:'抑制',associated:'関連',
  inspector:'証拠インスペクター',cellModel:'細胞モデル',context:'実験条件',version:'バージョン',provenance:'出典',contradictions:'反証・矛盾',noContradictions:'反証の登録なし',conflictHelp:'明示的に関連付けられた矛盾する記録です。正誤の自動判定ではありません。実験条件と原文を比較してください。',
  synthetic_source:'CausalMed 合成フィクスチャ',synthetic_fibroblast:'合成線維芽細胞',in_vitro_fixture:'In vitro テスト条件',synthetic_excerpt:'動作確認用に作成した架空の関係です。生物学的な観測結果や、検証された因果効果ではありません。',
  syntheticNotice:'合成データ・生物学的証拠ではありません',importedNotice:'取り込み記録・出典は未検証',notes:'研究ノート',fixtureNote:'このワークスペースには動作確認用の合成データを含みます。検証済みの治療標的はありません。',recentActivity:'最近の操作',noActivity:'プレビューに保存済みの履歴はありません。',
  import:'証拠を取り込む',importHelp:'CausalMed 形式の証拠 JSON（最大 2 MiB）を取り込みます。出典・原文・実験条件が必要です。',export:'ワークスペースを書き出す',exportHelp:'仮説・出典・評価の分割記録を含む JSON スナップショットを保存します。',imported:'証拠を取り込み',hypothesisCreated:'仮説を作成',statusUpdated:'仮説を更新',benchmarkCompleted:'評価を実行',
  createHypothesis:'仮説を作成',hypothesisTitle:'仮説スタジオ',hypothesisHelp:'特定の証拠から、反証可能な研究仮説を作成します。下書きの作成によって関係が確定した因果に変更されることはありません。',newHypothesis:'新しい仮説',title:'タイトル',interventionPlan:'介入計画',falsification:'反証条件',
  titlePlaceholder:'検証可能な研究上の問い',interventionPlaceholder:'対象・操作・細胞条件・対照群',falsificationPlaceholder:'どのような独立実験の結果が、この仮説を否定しますか？',saveDraft:'下書きを保存',cancel:'キャンセル',close:'閉じる',draft:'下書き',planned:'実験予定',archived:'アーカイブ',
  safetyUnknown:'安全性不明',unvalidated:'未検証',noHypotheses:'仮説はまだありません',noHypothesesHint:'証拠から関係を選び、検証可能な仮説を作成してください。',goEvidence:'証拠を見る',noResults:'一致する関係がありません',clearFilters:'絞り込みを解除',
  dockRight:'右にドッキング',dockBottom:'下にドッキング',floatPanel:'インスペクターを分離',panelHelp:'見出しをドッキング領域へドラッグするか、配置ボタンを使ってください。',panelDetached:'インスペクターを別ウインドウで表示中',restorePanel:'インスペクターを戻す',
  graph:'GPU で関係を表示',graphHelp:'選択した関係のネイティブ GPU プレビューです。左が起点、右が対象。実線は介入実験、破線は観測・仮説です。グラフ全体の探索は今後実装します。',
  datasetsTitle:'出典ライブラリ',datasetsHelp:'H5AD の形状・注釈と、証拠 JSON の出典を確認できます。発現値 QC をプレビューできます。モデル評価への接続は今後実装します。',license:'ライセンス',locator:'出典の場所',checksum:'取り込み SHA-256',importedAt:'取り込み日時',records:'件の関係',sources:'件の出典',sourceUnverified:'出典未検証',syntheticData:'合成データ',
  benchmarkTitle:'モデル評価',benchmarkHelp:'M0 は学習側の対照群平均を予測値とします。細胞測定をドナーごとに集約し、各ドナーを同じ重みで評価します。合成データによる動作確認であり、医学的な予測精度の検証ではありません。',
  runBenchmark:'合成 M0 を実行',benchmarkEmpty:'評価はまだありません',benchmarkHint:'同梱の合成データで評価パイプラインを確認できます。',baseline:'対照群平均ベースライン',split:'評価分割',splitValue:'データセット・介入対象・ドナーを分離',model:'モデル',mae:'ドナー単位 MAE',controlMean:'学習側の対照群平均',trainDonors:'学習側の対照ドナー',testDonors:'評価ドナー',arbitraryUnits:'任意単位',heldOut:'未知の介入対象',manifest:'分割記録',trainSamples:'学習サンプル ID',testSamples:'評価サンプル ID',
  abstained:'予測を保留',insufficient_independent_donors:'対照側・評価側それぞれに独立したドナーが 2 名以上必要です。',single_condition_required:'一致する細胞条件を一つ指定してください。',baselineOnly:'M0 のみ・モデルの優位性は未検証',loading:'ワークスペースを開いています…',busy:'処理中…',retry:'再試行',operationFailed:'処理を完了できませんでした。最後に保存した状態は保持されています。',desktop_required:'この操作はデスクトップアプリで利用できます。',
  import_too_large:'ファイルが 2 MiB を超えています。小さな証拠 JSON に分けてください。',already_imported:'この証拠ファイルはすでに取り込み済みです。',duplicate_source:'出典 ID が重複しています。新しい版には一意の ID を指定してください。',duplicate_edge:'関係 ID が重複しています。一意の ID を指定してください。',missing_source:'関係に指定された出典が見つかりません。',invalid_source:'出典の情報が不足しているか、長すぎます。',invalid_source_locator:'HTTPS の出典 URL、または合成データ用の fixture URI を指定してください。',invalid_edge:'分子名・実験条件・原文のいずれかが不足しています。',invalid_contradiction:'反証は、同じ起点・対象分子を持つ別の記録を指定してください。',observation_cannot_assert_causality:'観測データの関係には「関連」を使ってください。活性化・抑制とは断定できません。',unsupported_schema:'未対応のスキーマです。schemaVersion は 1 を指定してください。',invalid_bundle_size:'空のファイル、または初期版の上限（10,000 関係）を超えています。',invalid_json:'証拠 JSON の形式が一致しません。fixtures/ のサンプルを確認してください。',hypothesis_fields_required:'タイトル・介入計画・反証条件を入力してください。',edge_not_found:'関係が見つかりません。別の記録を選んでください。',hypothesis_not_found:'仮説が見つかりません。',copy:'コピー',copied:'コピー済み',blankSelection:'関係を選択してください',revision:'リビジョン',
};
const zh: Messages = {
  evidence:'证据',datasets:'数据集',hypotheses:'假设',benchmark:'基准评估',research:'IPF 研究',local:'本地工作区',preview:'只读预览',previewHelp:'请在桌面应用中导入、保存、运行评估和分离面板。此预览不保存研究数据。',synthetic:'合成数据',mixed:'混合来源',researchOnly:'仅供研究',
  evidenceTitle:'证据浏览器',evidenceHelp:'区分观察、干预实验与未验证假设。证据类型由导入者提供，来源尚未独立核验。',search:'搜索分子或关系',searchLabel:'搜索关系',all:'全部',observation:'观察',intervention:'干预实验',hypothesis:'假设',source:'起点',relation:'关系',target:'目标',kind:'证据',activates:'激活',inhibits:'抑制',associated:'相关',
  inspector:'证据检查器',cellModel:'细胞模型',context:'实验条件',version:'版本',provenance:'来源',contradictions:'反证与矛盾',noContradictions:'尚无矛盾记录',conflictHelp:'这些是明确关联的矛盾记录，并非自动判断。请比较实验条件与原文。',synthetic_source:'CausalMed 合成测试数据',synthetic_fibroblast:'合成成纤维细胞',in_vitro_fixture:'体外测试条件',synthetic_excerpt:'人工编写的测试关系，并非生物学观察或已验证的因果效应。',syntheticNotice:'合成数据 · 非生物学证据',importedNotice:'导入记录 · 来源未核验',notes:'研究笔记',fixtureNote:'此工作区包含合成测试数据。尚无经过验证的治疗靶点。',recentActivity:'最近操作',noActivity:'预览中没有已保存的操作记录。',
  import:'导入证据',importHelp:'导入 CausalMed 证据 JSON（最大 2 MiB），需包含来源、原文与实验条件。',export:'导出工作区',exportHelp:'保存包含假设、来源与评估划分记录的 JSON 快照。',imported:'已导入证据',hypothesisCreated:'已创建假设',statusUpdated:'已更新假设',benchmarkCompleted:'已运行评估',createHypothesis:'创建假设',hypothesisTitle:'假设工作室',hypothesisHelp:'从特定证据建立可证伪的研究问题。创建草稿不会将关系升级为已确认的因果。',newHypothesis:'新假设',title:'标题',interventionPlan:'干预计划',falsification:'证伪条件',titlePlaceholder:'可检验的研究问题',interventionPlaceholder:'目标、操作、细胞条件与对照组',falsificationPlaceholder:'什么独立实验结果能够推翻该假设？',saveDraft:'保存草稿',cancel:'取消',close:'关闭',draft:'草稿',planned:'计划实验',archived:'已归档',safetyUnknown:'安全性未知',unvalidated:'未验证',noHypotheses:'暂无假设',noHypothesesHint:'在证据中选择关系，建立可检验的假设。',goEvidence:'浏览证据',noResults:'没有匹配的关系',clearFilters:'清除筛选',
  dockRight:'停靠右侧',dockBottom:'停靠下方',floatPanel:'分离检查器',panelHelp:'将标题拖至停靠区域，或使用停靠按钮。',panelDetached:'检查器已在独立窗口中显示',restorePanel:'停靠检查器',graph:'打开 GPU 关系预览',graphHelp:'当前关系的原生 GPU 预览。左侧为起点，右侧为目标。实线代表干预实验，虚线代表观察或假设。完整图谱探索功能尚在计划中。',
  datasetsTitle:'来源库',datasetsHelp:'可查看 H5AD 维度、注释及证据 JSON 来源。可预览表达值 QC，模型评估连接将在后续实现。',license:'许可证',locator:'来源位置',checksum:'导入 SHA-256',importedAt:'导入时间',records:'条关系',sources:'个来源',sourceUnverified:'来源未核验',syntheticData:'合成数据',benchmarkTitle:'模型评估',benchmarkHelp:'M0 以训练对照组均值作为预测。细胞测量按供体汇总，各供体权重相同。这是合成数据流程测试，不是医学预测能力验证。',runBenchmark:'运行合成 M0',benchmarkEmpty:'尚无评估记录',benchmarkHint:'使用内置合成数据验证评估流程。',baseline:'对照均值基线',split:'评估划分',splitValue:'隔离数据集、干预目标与供体',model:'模型',mae:'供体 MAE',controlMean:'训练对照均值',trainDonors:'训练对照供体',testDonors:'测试供体',arbitraryUnits:'任意单位',heldOut:'未见干预目标',manifest:'划分记录',trainSamples:'训练样本 ID',testSamples:'测试样本 ID',abstained:'暂不预测',insufficient_independent_donors:'对照与测试各需要至少两个独立供体。',single_condition_required:'需要一个匹配的细胞条件。',baselineOnly:'仅 M0 · 尚未证明模型优势',loading:'正在打开工作区…',busy:'处理中…',retry:'重试',operationFailed:'操作未完成，最后保存的工作区仍保留。',desktop_required:'请在桌面应用中使用此操作。',
  import_too_large:'文件超过 2 MiB，请拆分为较小的证据 JSON。',already_imported:'此证据文件已经导入。',duplicate_source:'来源 ID 重复，请为新版本指定唯一 ID。',duplicate_edge:'关系 ID 重复，请使用唯一 ID。',missing_source:'关系引用的来源不存在。',invalid_source:'来源元数据不完整或过长。',invalid_source_locator:'请使用 HTTPS 来源链接或合成数据的 fixture URI。',invalid_edge:'关系缺少分子、实验条件或原文。',invalid_contradiction:'矛盾记录必须引用起点与目标相同的另一条关系。',observation_cannot_assert_causality:'观察数据必须使用“相关”关系，不能断言激活或抑制。',unsupported_schema:'不支持的格式版本，请使用 schemaVersion 1。',invalid_bundle_size:'文件为空或超过初版工作区上限（10,000 条关系）。',invalid_json:'JSON 不符合证据格式，请参考 fixtures/ 中的示例。',hypothesis_fields_required:'请填写标题、干预计划与证伪条件。',edge_not_found:'关系不存在，请选择其他记录。',hypothesis_not_found:'未找到该假设。',copy:'复制',copied:'已复制',blankSelection:'请选择一条关系',revision:'修订',
};
const messages: Record<Locale,Messages> = { en, ja, zh };
export type TranslationKey = keyof Messages;
export function translator(locale: Locale) { return (key: string): string => messages[locale][key as TranslationKey] ?? key; }
export function errorMessage(error: unknown, locale: Locale): string {
  const raw = String(error); const code = raw.replace(/^Error: /, '').split(':')[0];
  return code in en ? translator(locale)(code) : translator(locale)('operationFailed');
}
export function formatDate(value: string, locale: Locale) { return new Intl.DateTimeFormat({en:'en-US',ja:'ja-JP',zh:'zh-CN'}[locale], { dateStyle:'short', timeStyle:'short' }).format(new Date(value)); }
export const localeLabels: Record<Locale,string> = { en:'English',ja:'日本語',zh:'简体中文' };

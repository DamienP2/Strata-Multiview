// TypeScript mirror of crates/cg-core/src/model.rs — keep in sync.

export type Color = [number, number, number, number];
export type Value = number | Color;

export type Easing = { type: 'linear' } | { type: 'hold' } | { type: 'easeIn' } | { type: 'easeOut' } | { type: 'easeInOut' } | { type: 'bezier'; x1: number; y1: number; x2: number; y2: number } | { type: 'back' } | { type: 'elastic' } | { type: 'bounce' };

export interface Keyframe {
  frame: number;
  value: Value;
  easing: Easing;
}

export const PROPS = ['x', 'y', 'width', 'height', 'anchorX', 'anchorY', 'rotation', 'scaleX', 'scaleY', 'opacity', 'fill', 'stroke', 'strokeWidth', 'cornerRadius', 'fontSize', 'letterSpacing', 'cropLeft', 'cropRight', 'cropTop', 'cropBottom', 'arcStart', 'arcEnd', 'innerRadius', 'shadowColor', 'shadowX', 'shadowY', 'shadowBlur', 'drawStart', 'drawEnd', 'cornerTl', 'cornerTr', 'cornerBr', 'cornerBl', 'fill2'] as const;
export type Prop = (typeof PROPS)[number];

export const COLOR_PROPS: ReadonlySet<Prop> = new Set(['fill', 'stroke', 'shadowColor', 'fill2']);

export type TextAlign = 'left' | 'center' | 'right';
export type VerticalAlign = 'top' | 'middle' | 'bottom';
export type ImageFit = 'contain' | 'cover' | 'stretch';

/** Text styling shared by the text-based widgets. */
export type TextScroll = 'none' | 'horizontal' | 'vertical';
export type TextTransform = 'none' | 'upper' | 'lower' | 'capitalize';
export type TextDecoration = 'none' | 'underline' | 'lineThrough';

export interface FontStyle {
  fontFamily: string;
  fontWeight: number;
  align: TextAlign;
  verticalAlign: VerticalAlign;
  uppercase: boolean;
}

export type ClockMode = 'time' | 'countdown' | 'elapsed' | 'timer';

export type LayerKind =
  | { type: 'group'; clip?: boolean; /** A Lottie animation placed by the Lottie widget: its file ('' = not chosen yet). */ lottie?: string }
  | ({ type: 'rect' } & import('../widgets/quad').Config)
  | ({ type: 'ellipse' } & import('../widgets/ellipse').Config)
  | (({
      type: 'text';
      text: string;
      /** Paragraph: lines wrap at the box width. */
      wrap?: boolean;
      transform?: TextTransform;
      decoration?: TextDecoration;
      /** In font sizes (default 1.2). */
      lineHeight?: number;
      /** Fixed, or scrolling like a ticker (horizontal) or a credit roll (vertical). */
      scroll?: TextScroll;
      /** px/s. */
      speed?: number;
      separator?: string;
      looping?: boolean;
      /** When the text does not fit its box (default fit). */
      overflow?: TextOverflow;
      /** Fit: narrowest (fraction of the width), then smallest font size (fraction), default 0.8 and 0.6. */
      squeeze?: number;
      minSize?: number;
      /** The Operator warns under this (fraction, default 0.8). */
      warnBelow?: number;
    } & FontStyle) &
      import('../widgets/text').Config)
  | ({ type: 'image'; src: string; fit: ImageFit } & import('../widgets/image').Config)
  | (({ type: 'clock'; mode: ClockMode; format: string; timezone: string; target: string } & FontStyle) & import('../widgets/timer').Config)
  | (({ type: 'ticker'; text: string; separator: string; speed: number } & FontStyle) & import('../widgets/ticker').Config)
  | ({ type: 'roll'; text: string; speed: number; loop: boolean } & FontStyle)
  | (({ type: 'counter'; from: string; to: string; duration: number; decimals: number; prefix: string; suffix: string; grouping: boolean } & FontStyle) & import('../widgets/counter').Config)
  | ({ type: 'progress'; value: string; max: string; duration: number; track: string } & import('../widgets/progress').Config)
  | ({ type: 'rotator'; text: string; interval: number; fade: number } & FontStyle)
  | (({ type: 'typewriter'; text: string; speed: number; cursor: boolean } & FontStyle) & import('../widgets/typewriter').Config)
  | { type: 'slideshow'; images: string; interval: number; fade: number; fit: ImageFit }
  | ({ type: 'qrCode'; data: string; background: string } & import('../widgets/qrcode').Config)
  | ({ type: 'web' } & import('../widgets/web').Config)
  | ({ type: 'socialWall' } & import('../widgets/socialwall').Config)
  | ({ type: 'socialBadge' } & import('../widgets/socialbadge').Config)
  | ({ type: 'barChart'; text: string; percent: boolean; duration: number; track: string; textColor: string } & FontStyle)
  | ({ type: 'polygon' } & import('../widgets/polygon').Config)
  | ({ type: 'pen' } & import('../widgets/pen').Config)
  | ({ type: 'table' } & import('../widgets/table').Config)
  | ({ type: 'credits' } & import('../widgets/credits').Config)
  | ({ type: 'chart' } & import('../widgets/chart').Config)
  | ({ type: 'map' } & import('../widgets/map').Config)
  | ({ type: 'scoreboard' } & import('../widgets/scoreboard').Config);

export type LayerType = LayerKind['type'];

export type Layer = LayerKind & {
  id: string;
  name: string;
  parent?: string | null;
  visible: boolean;
  locked: boolean;
  /** Timed widgets wait for the operator to start them (on Continue, or on `startTrigger` when `startOn` is 'trigger'). */
  manualStart?: boolean;
  startOn?: 'continue' | 'trigger';
  startTrigger?: string;
  /** Scale X and Y change together. */
  scaleLock?: boolean;
  /** Width and height keep their ratio when resized (fields and stage handles). */
  lockRatio?: boolean;
  /** Drop shadow under the layer and its children (props shadowColor / shadowX / shadowY / shadowBlur); not for Texts (their own). */
  dropShadow?: boolean;
  /** How a new value of the data it shows (a source, a data set) shows on air, by data key (set at its port in the Data canvas); none = at once. */
  liveChanges?: Record<string, LiveChange>;
  /** When the layer is shown, in frames of the template's timeline (its bar in the timeline); none = all along. */
  span?: { from: number; to: number };
  /** A movement repeated while the layer is on air (Designer › Animations › Loop); runs on the layer's age, also once the template holds. */
  motion?: Motion;
  /** Properties on the Designer's timeline (animatable); none = position, size, opacity. */
  timeline?: Prop[];
  props: Partial<Record<Prop, Value>>;
  tracks: Partial<Record<Prop, Keyframe[]>>;
  bindings: Partial<Record<Prop, string>>;
  /** Settings exposed to the Operator: dotted path in the layer → data field key. */
  settingBindings?: Record<string, string>;
  /** Per-format changes (9:16…), by format id. */
  overrides?: Record<string, LayerOverride>;
};

/** How a layer differs in one format; anything absent is inherited. */
export interface LayerOverride {
  props?: Partial<Record<Prop, Value>>;
  tracks?: Partial<Record<Prop, Keyframe[]>>;
  visible?: boolean;
}

/** Alternative layout of a project; the main one is width × height ("main"). */
export interface Format {
  id: string;
  name: string;
  width: number;
  height: number;
}

export const MAIN_FORMAT = 'main';

export type MarkerKind = 'pause' | 'out' | 'label';

export interface Marker {
  id: string;
  frame: number;
  name: string;
  /** `out`: the On air point, where the out animation starts (one per template). */
  kind: MarkerKind;
  /** How a pause resumes (default: the operator's Continue). */
  resume?: Resume;
}

export type Resume = { type: 'manual' } | { type: 'after'; seconds: number } | { type: 'trigger'; name: string } | { type: 'triggers' };
/** What an event node of the Triggers watches on air (the server). */
export type TriggerEvent =
  | { kind: 'scrollEnd'; layer: string; passes?: number }
  | { kind: 'timerZero'; layer: string }
  | { kind: 'timerAt'; layer: string; seconds: number }
  | { kind: 'videoEnd'; layer: string }
  | { kind: 'lastRow'; dataset: string }
  | { kind: 'dataChange'; key: string }
  | { kind: 'dataIs'; key: string; op: string; value: string }
  | { kind: 'graphic'; template: string; out?: boolean };

/** Marker model of templates made by this version (see Template.markerModel). */
export const MARKER_MODEL = 2;

export type FieldKind = 'text' | 'multiline' | 'number' | 'color' | 'image' | 'boolean' | 'table';

export interface Field {
  key: string;
  label: string;
  kind: FieldKind;
  default: string;
  /** Live value from a data source. */
  binding?: FieldBinding | null;
  /** Range of a number (slider). */
  min?: number;
  max?: number;
  /** Choices of an exposed setting (value, label): the Operator picks one. */
  options?: [string, string][];
}

/** A data source and a path in its value (`rows[{row}].name`, `items[*].title`…). */
export interface FieldBinding {
  source: string;
  path: string;
  /** Applied in order to the value read (mirror of model::DataFilter). */
  filters?: DataFilter[];
  /** Sorts a list of the source before the path reads it. */
  sort?: DataSort;
  /** Table pipeline (compiled from the Data canvas): the list read as rows, then the steps. */
  list?: string;
  steps?: DataStep[];
  /** How a new value of the source shows on air, set before it was the layers' (Layer.liveChange wins). */
  change?: ValueChange;
  /** Length of that change, in seconds (0.6 when unset). */
  changeSecs?: number;
}

/** How a value its data source changed shows on air (mirror of model::ValueChange). */
export type ValueChange = 'cut' | 'fade' | 'slide' | 'flip' | 'count' | 'outIn' | 'hold';
/** How a layer shows a new value of its data on air (mirror of model::LiveChange). */
export interface LiveChange {
  kind: ValueChange;
  /** Its length (0.6 s when unset). */
  secs?: number;
}
/** A value moving on air in the layers showing it (mirror of playout::Change). */
export interface ValueMove {
  key: string;
  kind: ValueChange;
  /** The layer moving (slide, flip); none: every layer showing the value. */
  layer?: string;
  from: string;
  frame: number;
  length: number;
}

/** A step of the table pipeline (mirror of model::DataStep). */
export type DataStep = { type: 'filter'; where: Condition[]; any?: boolean; from: number; count: number; columns: string[] } | { type: 'sort'; keys: SortKey[] } | { type: 'transform'; columns: string[]; ops: DataFilter[] };

export type CondOp = 'eq' | 'ne' | 'contains' | 'notContains' | 'startsWith' | 'gt' | 'ge' | 'lt' | 'le' | 'empty' | 'notEmpty';
export interface Condition {
  column: string;
  op: CondOp;
  value: string;
}
export interface SortKey {
  column: string;
  descending?: boolean;
  numeric?: boolean;
}

/** Too long for its box: narrower then smaller, "…", cut, or drawn outside. */
export type TextOverflow = 'fit' | 'ellipsis' | 'clip' | 'visible';

/** How the graphic on air goes from one row of a data set to the next. */
export type RowChange = 'outIn' | 'fade' | 'cut';

/** Rows the operator steps through (mirror of model::DataSet); compiled from a Data set node. */
export interface DataSet {
  key: string;
  name: string;
  source: string;
  list: string;
  steps?: DataStep[];
  change?: RowChange;
}

/** Data keys of a data set: its row, its number of rows, each column of the current row. */
export const rowKey = (key: string) => `${key}__row`;
export const countKey = (key: string) => `${key}__count`;
/** All the rows of a data set, for widgets showing a table: JSON `[[header…], [cells…], …]`. */
export const tableKey = (key: string) => `${key}__table`;
/** Its rows as text lines, for a Text; only the current row when `oneKey` is "1" (chosen in the Operator). */
export const linesKey = (key: string) => `${key}__lines`;
export const oneKey = (key: string) => `${key}__one`;
export const columnKey = (key: string, column: string) => `${key}_${column.replace(/[^A-Za-z0-9]/g, '_')}`;

const usedCache = new WeakMap<object, DataSet[]>();
/** The data sets the graphic really reads: a key of theirs (row, column, all rows) appears in its layers,
 * variables or logic. A data set node left in the Data canvas, wired to nothing, is not shown to the Operator. */
export function usedDatasets(t: Pick<Template, 'datasets' | 'layers' | 'fields' | 'logic'>): DataSet[] {
  const sets = t.datasets ?? [];
  if (!sets.length) return sets;
  const hit = usedCache.get(t);
  if (hit) return hit;
  const text = JSON.stringify([t.layers, t.fields, t.logic ?? null]);
  const out = sets.filter((d) => text.includes(`${d.key}_`));
  usedCache.set(t, out);
  return out;
}

/** A node of the Data canvas graph: a source's list, a step, or a data set. */
export type FlowNode = { id: string; x: number; y: number } & ({ type: 'source'; source: string; list: string } | DataStep | { type: 'dataset'; key: string; name: string; change: RowChange });
/** The Data canvas graph; an edge goes to a node id or to `field:<key>`. */
export interface DataFlow {
  nodes: FlowNode[];
  edges: { from: string; to: string }[];
}

/** Sorts the list at `list` (dotted keys, "" = the root) by `by` (mirror of model::DataSort). */
export interface DataSort {
  list: string;
  by: string;
  descending?: boolean;
  numeric?: boolean;
}

export type FilterOp = 'add' | 'sub' | 'mul' | 'div';

export type DataFilter =
  | { type: 'upper' }
  | { type: 'lower' }
  | { type: 'capitalize' }
  | { type: 'trim' }
  | { type: 'prefix'; text: string }
  | { type: 'suffix'; text: string }
  | { type: 'replace'; find: string; with: string }
  | { type: 'truncate'; max: number }
  | { type: 'lines'; from: number; count: number }
  | { type: 'join'; separator: string }
  | { type: 'number'; decimals: number; grouping: boolean }
  | { type: 'math'; op: FilterOp; value: number }
  | { type: 'default'; text: string }
  | { type: 'map'; when: string; then: string };

export type DataSourceKind = { type: 'json'; url: string; headers?: Record<string, string> } | { type: 'csv'; url: string; header?: boolean } | { type: 'rss'; url: string } | { type: 'xml'; url: string; headers?: Record<string, string> } | { type: 'push'; format?: 'json' | 'csv' | 'xml' };

export type DataSource = DataSourceKind & {
  id: string;
  name: string;
  /** The project it belongs to; unset = shared by every project (main menu > Data). */
  project?: string;
  /** Seconds between fetches. */
  interval: number;
  enabled: boolean;
};

export interface DataStatus {
  updatedAt?: number | null;
  checkedAt?: number | null;
  error?: string | null;
  bytes?: number;
}

export type DataSourceInfo = DataSource & DataStatus;

export interface Template {
  id: string;
  name: string;
  duration: number;
  markers: Marker[];
  fields: Field[];
  layers: Layer[];
  /** Default zone (id). */
  zone: string;
  /** Relations between values (node graph). */
  logic?: Logic;
  /** What happens on air: events → out, a pause playing on, a trigger, the next row (the server runs it). */
  triggers?: Logic;
  /** The Data canvas graph; the fields' bindings and the data sets are compiled from it. */
  dataflow?: DataFlow;
  datasets?: DataSet[];
  /** Published revision (0: never published). */
  revision?: number;
  /** Marker model (MARKER_MODEL); older templates are converted by the server. */
  markerModel?: number;
}

export type TextMetric = 'width' | 'height' | 'lines' | 'chars';
export type MathOp = 'add' | 'sub' | 'mul' | 'div' | 'min' | 'max' | 'abs' | 'round' | 'floor' | 'ceil';
export type Cmp = 'gt' | 'ge' | 'lt' | 'le' | 'eq' | 'ne' | 'empty' | 'notEmpty';
export type TargetMode = 'set' | 'add' | 'multiply';

export type LogicNodeKind =
  | { type: 'layerProp'; layer: string; prop: Prop }
  | { type: 'textSize'; layer: string; metric: TextMetric }
  | { type: 'field'; key: string }
  | { type: 'constant'; value: number }
  | { type: 'math'; op: MathOp }
  | { type: 'clamp'; min: number; max: number }
  | { type: 'condition'; cmp: Cmp }
  | { type: 'target'; layer: string; prop: Prop; mode: TargetMode }
  | { type: 'text'; value: string }
  | { type: 'event'; on: TriggerEvent }
  | { type: 'bool'; op: 'and' | 'or' }
  | { type: 'out' }
  | { type: 'resume'; marker: string }
  | { type: 'fire'; name: string }
  | { type: 'nextRow'; dataset: string };

export type LogicNode = LogicNodeKind & { id: string; x: number; y: number };

export interface LogicEdge {
  from: string;
  to: string;
  /** Input port of `to`: a, b, in, then, else. */
  port: string;
}

export interface Logic {
  nodes: LogicNode[];
  edges: LogicEdge[];
}

export type ZoneTransition = 'sequential' | 'cut' | 'fade';

/** Screen area showing one template at a time; `layer` is the stacking order. */
export interface Zone {
  id: string;
  name: string;
  layer: number;
  color: string;
  transition: ZoneTransition;
  /** Area by format id, in fractions of the frame (designer guide). */
  areas?: Record<string, Area>;
}

/** A rectangle in fractions of the frame (0..1). */
export interface Area {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface RundownItem {
  id: string;
  name: string;
  templateId: string;
  /** Zone id ("" = the template's zone). */
  zone: string;
  /** Playlist (rundown section) id. */
  playlist: string;
  /** Legacy playout layer, kept in step with the zone by the server. */
  layer: number;
  data: Record<string, string>;
  /** When bound fields read their source (default: live). */
  read?: ReadMode;
  /** Auto next (playlist set to auto next): seconds on air before the next graphic; none = the operator takes it. */
  autoSecs?: number;
  /** Stays on air when the next graphic is taken (in another zone). */
  hold?: boolean;
}

/** `live`: on take and on every change of the source; `take`: once, on take; `manual`: when the operator asks. */
export type ReadMode = 'live' | 'take' | 'manual';

export interface FrameRate {
  num: number;
  den: number;
}

export interface Project {
  id: string;
  name: string;
  description?: string;
  width: number;
  height: number;
  frameRate: FrameRate;
  /** Language of the texts (hyphenation, French spaces), "fr" by default. */
  language?: string;
  /** Length of a new template (its IN and OUT), seconds; each one is then set in the timeline. None: 1 s. */
  templateSecs?: number;
  /** The project's own palette (the shared ones are in the Library). */
  colors?: NamedColor[];
  templates: Template[];
  rundown: RundownItem[];
  zones: Zone[];
  playlists: Playlist[];
  formats: Format[];
  /** Designer drafts not yet published (the operator uses `templates`). */
  drafts?: Template[];
  /** The empty template a new project starts with: the server removes it at a publish while it has no layer. */
  starter?: string;
  /** Designer aids (mirror of model::Guides). */
  guides?: Guides;
  /** The Designer's order of the templates (ids); the others follow. */
  designOrder?: string[];
}

/** Guides, safe areas and areas to keep clear; fractions of the picture (0..1). */
export interface Guides {
  vertical: number[];
  horizontal: number[];
  actionSafe: number;
  titleSafe: number;
  /** Safe areas placed by hand (instead of the margins). */
  actionArea?: Area;
  titleArea?: Area;
  exclusions: Exclusion[];
}

export interface Exclusion {
  id: string;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Playlist {
  id: string;
  name: string;
  /** The graphics go on to the next one by themselves (RundownItem.autoSecs). */
  autoNext?: boolean;
  /** After the last graphic, the first again. */
  loop?: boolean;
}

export type Phase = 'playing' | 'holding' | 'out' | 'outHold';

export interface PlayoutItem {
  templateId: string;
  data: Record<string, string>;
  frame: number;
  target: number;
  phase: Phase;
  rundownId?: string | null;
  /** Project frames since the take (keeps counting while holding). */
  age?: number;
  /** Frames spent holding at the current stop. */
  held?: number;
  /** Waiting for this graphic to finish going out (sequential zones). */
  next?: { templateId: string; data: Record<string, string>; rundownId?: string | null; target: number } | null;
  /** Widgets started, paused or reset by the operator, by layer id. */
  clocks?: Record<string, LayerClock>;
  /** What the operator changed in the spec widgets (web/src/widgets), by layer id. */
  widgets?: Record<string, unknown>;
  /** A cross-fade from the data it had (drawn fading out above it). */
  fade?: { data: Record<string, string>; frame: number; length: number } | null;
  /** Values a data source changed, moving in the layers showing them. */
  changes?: ValueMove[];
  /** The graphic it replaced in a « Fade » zone, fading out under it. */
  leaving?: { item: PlayoutItem; frame: number; length: number } | null;
}

/** Own time of a widget layer, in frames on air. */
export interface LayerClock {
  offset: number;
  pausedAt?: number | null;
}

export type WidgetControl = 'start' | 'pause' | 'toggle' | 'reset' | 'restart';

export interface ChannelState {
  layers: Record<string, PlayoutItem>;
}

/** Commands address a zone by id, or a raw playout layer. */
type Target = { zone?: string; layer?: number };

export type Command =
  | ({ action: 'take'; templateId: string; data?: Record<string, string>; rundownId?: string } & Target)
  | ({ action: 'continue' } & Target)
  | ({ action: 'takeOut' } & Target)
  | ({ action: 'update'; data: Record<string, string>; fade?: boolean } & Target)
  | ({ action: 'widget'; target: string; control: WidgetControl } & Target)
  /** An operator action on a spec widget of the graphic on air (see web/src/widgets). */
  | ({ action: 'act'; target: string; op: string; args?: unknown } & Target)
  /** Another row of a data set of the graphic on air (1 = first). */
  | ({ action: 'row'; dataset: string; row: number } & Target)
  | ({ action: 'clear' } & Target)
  | { action: 'trigger'; name: string };

export type Data = Record<string, string>;

export interface ProjectSummary {
  id: string;
  name: string;
  description: string;
  width: number;
  height: number;
  frameRate: FrameRate;
  templates: number;
  rundown: number;
  /** Template shown on the card. */
  cover?: string;
  /** Last saved (ms since 1970). */
  modifiedAt?: number;
}

export interface ChannelInfo {
  id: string;
  name: string;
  width: number;
  height: number;
  frameRate: FrameRate;
  /** Two fields a frame (the rate counts fields). */
  interlaced?: boolean;
  projectId: string | null;
  /** What the channel goes out to: "SDI · name", "NDI · name"… */
  outputs: string[];
  renditions: { id: string; name: string; width: number; height: number; slate?: string; slateStyle?: string }[];
  /** Still picture the outputs show while no project is loaded ("" = none). */
  slate?: string;
  /** The design shown while no project is loaded ("custom": the still picture). */
  slateStyle?: string;
}

export type Role = 'admin' | 'designer' | 'operator' | 'viewer';

export interface User {
  id: string;
  username: string;
  displayName: string;
  role: Role;
  projects: string[] | null;
  channels: string[] | null;
  disabled: boolean;
  /** Language of the interface ("en", "fr"); empty = the browser's. */
  lang?: string;
  /** Colour theme of the interface ("dark", "light", "system"); empty = dark. */
  theme?: string;
}

/** Layer types whose text is styled with a FontStyle. */
export const TEXT_LIKE = new Set<LayerType>(['text', 'clock', 'ticker', 'roll', 'counter', 'rotator', 'typewriter', 'barChart']);

/** A colour of a palette: the layers using its value follow it when it changes. */
export interface NamedColor {
  id: string;
  name: string;
  /** `#rrggbb` or `#rrggbbaa`. */
  value: string;
}

/** A palette shared by every project (Library › Colours). */
export interface Palette {
  id: string;
  name: string;
  colors: NamedColor[];
}

/** A movement a layer repeats (mirror of model::Motion). */
export interface Motion {
  kind: MotionKind;
  /** One cycle, in seconds. */
  period: number;
  /** The frame from which it moves (the end of the in animation). */
  from: number;
}
export type MotionKind = 'pulse' | 'blink' | 'swing' | 'float' | 'shake' | 'kenBurns';

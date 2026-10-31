// TypeScript mirror of crates/strata-mv-core/src/model.rs — keep in sync.

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
  | ({ type: 'polygon' } & import('../widgets/polygon').Config)
  | ({ type: 'source' } & import('../widgets/source').Config)
  | ({ type: 'audioMeter' } & import('../widgets/broadcast').MeterConfig)
  | ({ type: 'loudness' } & import('../widgets/broadcast').LoudnessConfig)
  | ({ type: 'timecode' } & import('../widgets/broadcast').TimecodeConfig)
  | ({ type: 'status' } & import('../widgets/broadcast').StatusConfig)
  | ({ type: 'alarmLog' } & import('../widgets/broadcast').AlarmLogConfig);

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
/** Marker model of templates made by this version (see Template.markerModel). */
export const MARKER_MODEL = 2;

export type FieldKind = 'text' | 'multiline' | 'number' | 'color' | 'image' | 'boolean' | 'table';

export interface Field {
  key: string;
  label: string;
  kind: FieldKind;
  default: string;
  /** Range of a number (slider). */
  min?: number;
  max?: number;
  /** Choices of an exposed setting (value, label): the Operator picks one. */
  options?: [string, string][];
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

/** Too long for its box: narrower then smaller, "…", cut, or drawn outside. */
export type TextOverflow = 'fit' | 'ellipsis' | 'clip' | 'visible';

export interface Template {
  id: string;
  name: string;
  duration: number;
  markers: Marker[];
  fields: Field[];
  layers: Layer[];
  /** Default zone (id). */
  zone: string;
  /** Published revision (0: never published). */
  revision?: number;
  /** Marker model (MARKER_MODEL); older templates are converted by the server. */
  markerModel?: number;
  /** The multiview's background, under everything (none: a very dark grey). */
  background?: import('./backdrop').Backdrop;
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
  /** The project's own palette (the shared ones are in the Library). */
  colors?: NamedColor[];
  templates: Template[];
  zones: Zone[];
  formats: Format[];
  /** Designer drafts not yet published (the operator uses `templates`). */
  drafts?: Template[];
  /** Designer aids (mirror of model::Guides). */
  guides?: Guides;
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

export interface ChannelState {
  layers: Record<string, PlayoutItem>;
}

/** Commands address a zone by id, or a raw playout layer. */
type Target = { zone?: string; layer?: number };

export type Command = ({ action: 'take'; templateId: string } & Target) | ({ action: 'clear' } & Target);

export type Data = Record<string, string>;

export interface ProjectSummary {
  id: string;
  name: string;
  description: string;
  width: number;
  height: number;
  frameRate: FrameRate;
  templates: number;
  /** The Source tiles of the multiview. */
  tiles: number;
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
export const TEXT_LIKE = new Set<LayerType>(['text', 'clock']);

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

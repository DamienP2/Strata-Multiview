// The alarm settings (mirrors crates/strata-mv-core/src/alarm.rs: keep in sync) and what the Alarms
// page says of each fault.
import type { AlarmKind, Severity } from '../engine/liveInputs';

export interface Rule {
  enabled: boolean;
  /** Seconds the fault lasts before it is raised. */
  after: number;
  severity: Severity;
  threshold: number;
  /** What is expected (the colour space: an id of COLOR_SPACES). */
  expect?: string;
}

/** The colour spaces an input may be expected in (mirrors COLOR_SPACES). */
export const COLOR_SPACES: [string, string][] = [
  ['bt709', 'SDR BT.709 (BT.601 in SD)'],
  ['bt2020', 'SDR BT.2020'],
  ['pq', 'HDR10 PQ BT.2020'],
  ['hlg', 'HLG BT.2020'],
  ['hdr', 'HDR (PQ or HLG)'],
  ['any', 'Any'],
];

export interface Profile {
  id: string;
  name: string;
  rules: Partial<Record<AlarmKind, Rule>>;
}

export interface Webhook {
  id: string;
  name: string;
  enabled: boolean;
  method: 'GET' | 'POST';
  url: string;
  minSeverity: Severity;
  onEnd: boolean;
}

/** SNMP v2c traps (docs/STRATA-MV-MIB.txt). */
export interface SnmpTarget {
  id: string;
  name: string;
  enabled: boolean;
  host: string;
  community: string;
  minSeverity: Severity;
  onEnd: boolean;
}

/** Lines to a syslog server (RFC 5424). */
export interface SyslogTarget {
  id: string;
  name: string;
  enabled: boolean;
  host: string;
  transport: 'udp' | 'tcp';
  facility: number;
  minSeverity: Severity;
  onEnd: boolean;
  audit: boolean;
}

export interface AlarmSettings {
  profiles: Profile[];
  inputs: Record<string, string>;
  system: Partial<Record<AlarmKind, Rule>>;
  sound: Severity | null;
  webhooks: Webhook[];
  snmp: SnmpTarget[];
  snmpBaseOid: string;
  syslog: SyslogTarget[];
}

/** Mirrors strata_mv_core::alarm::SNMP_BASE_OID. */
export const SNMP_BASE_OID = '1.3.6.1.4.1.8072.9999.9999.3637';

export const DEFAULT_PROFILE = 'default';

/** The faults of an input, in the order of the settings (mirrors INPUT_ALARMS). */
export const INPUT_ALARMS: AlarmKind[] = [
  'loss',
  'onBackup',
  'black',
  'freeze',
  'fps',
  'reconnecting',
  'silence',
  'channelSilent',
  'loud',
  'quiet',
  'peak',
  'clipping',
  'phase',
  'noTimecode',
  'noCaptions',
  'colorSpace',
  'illegalLevels',
  'fullRange',
  'hdrMetadata',
  'noColorInfo',
  'tsPriority1',
  'tsPriority2',
];
/** The faults of the plant (mirrors SYSTEM_ALARMS). */
export const SYSTEM_ALARMS: AlarmKind[] = ['ptpUnlocked', 'ntpUnsynced', 'cpuHigh', 'memoryHigh', 'outputDropped'];

const CRITICAL: AlarmKind[] = ['loss', 'freeze', 'black', 'silence', 'ptpUnlocked'];

/** The rule of a fault unless set otherwise (mirrors default_rule). */
export function defaultRule(k: AlarmKind): Rule {
  const r = (enabled: boolean, after: number, threshold: number): Rule => ({
    enabled,
    after,
    severity: CRITICAL.includes(k) ? 'critical' : 'warning',
    threshold,
  });
  switch (k) {
    case 'loss':
      return r(true, 0, 0);
    case 'black':
      return r(true, 3, 0);
    case 'freeze':
      return r(true, 10, 0);
    case 'fps':
      return r(true, 5, 10);
    case 'reconnecting':
      return r(true, 0, 3);
    case 'onBackup':
      return r(true, 0, 0);
    case 'silence':
      return r(true, 5, -60);
    case 'channelSilent':
      return r(true, 10, -60);
    case 'loud':
      return r(true, 10, 3);
    case 'quiet':
      return r(true, 30, 10);
    case 'peak':
      return r(true, 0, -1);
    case 'clipping':
      return r(true, 0, -0.1);
    case 'phase':
      return r(true, 3, -0.3);
    case 'noTimecode':
      return r(false, 5, 0);
    case 'noCaptions':
      return r(false, 10, 0);
    case 'tsPriority1':
      return r(true, 0, 0);
    case 'tsPriority2':
      // Off unless asked: many IP contributions (ffmpeg, OBS) space their PCR more than 40 ms.
      return r(false, 0, 0);
    case 'ptpUnlocked':
      return r(true, 5, 0);
    case 'ntpUnsynced':
      return r(true, 60, 0);
    case 'cpuHigh':
    case 'memoryHigh':
      return r(true, 10, 90);
    case 'outputDropped':
      return r(true, 0, 5);
    case 'colorSpace':
      return { ...r(true, 5, 0), expect: 'bt709' };
    case 'illegalLevels':
      return r(true, 5, 1);
    case 'fullRange':
      return r(false, 5, 0);
    case 'hdrMetadata':
      return r(true, 10, 0);
    case 'noColorInfo':
      return r(false, 10, 0);
  }
}

/** The unit of a fault's threshold ("" none: mirrors threshold_unit). */
export function thresholdUnit(k: AlarmKind): string {
  switch (k) {
    case 'fps':
    case 'cpuHigh':
    case 'memoryHigh':
      return '%';
    case 'reconnecting':
      return '/min';
    case 'silence':
    case 'channelSilent':
    case 'clipping':
      return 'dBFS';
    case 'loud':
    case 'quiet':
      return 'LU';
    case 'peak':
      return 'dBTP';
    case 'phase':
      return 'corr.';
    case 'outputDropped':
      return '/10 s';
    case 'illegalLevels':
      return '% px';
    default:
      return '';
  }
}

/** How fault `k` of `input` is raised (mirrors AlarmSettings::rule). */
export function ruleOf(s: AlarmSettings, input: string, k: AlarmKind): Rule {
  if (SYSTEM_ALARMS.includes(k)) return s.system[k] ?? defaultRule(k);
  const pid = s.inputs[input] ?? DEFAULT_PROFILE;
  const p = s.profiles.find((x) => x.id === pid) ?? s.profiles[0];
  return p?.rules[k] ?? defaultRule(k);
}

/** What each fault is (English source, translated with t()). */
export const KIND_INFO: Record<AlarmKind, [string, string]> = {
  loss: ['Signal loss', 'No picture comes.'],
  black: ['Black', 'The picture is black.'],
  freeze: ['Frozen picture', 'A picture that moved stops (only noise left), or repeats to the pixel; never colour bars or a plain picture.'],
  fps: ['Abnormal frame rate', 'Pictures a second away from the format’s rate by more than the threshold.'],
  reconnecting: ['Reconnecting', 'The input stopped and started again this many times in a minute.'],
  onBackup: ['On backup', 'The input lost its signal: its tiles show its backup input.'],
  silence: ['Silence', 'Every audio channel under the threshold.'],
  channelSilent: ['Channel silent', 'One channel under the threshold while another is heard (the LFE of 5.1 aside).'],
  loud: ['Too loud', 'Short-term loudness above the target (−23 LUFS) by more than the threshold.'],
  quiet: ['Too quiet', 'Short-term loudness under the target by more than the threshold (not silent).'],
  peak: ['True peak', 'True peak above the threshold.'],
  clipping: ['Clipping', 'Sample peak reaching the threshold.'],
  phase: ['Out of phase', 'Phase correlation of left and right under the threshold.'],
  noTimecode: ['No timecode', 'The pictures carry no timecode.'],
  noCaptions: ['No closed captions', 'The pictures carry no A/53 closed captions.'],
  tsPriority1: ['TR 101 290 priority 1', 'Transport stream: sync, PAT, continuity, PMT or PID errors.'],
  tsPriority2: ['TR 101 290 priority 2', 'Transport stream: transport, CRC or PCR errors.'],
  ptpUnlocked: ['PTP unlocked', 'ptp4l runs but follows no grandmaster.'],
  ntpUnsynced: ['NTP not synchronised', 'The system clock is not synchronised by NTP.'],
  cpuHigh: ['Processors busy', 'The machine’s processors above the threshold (over 10 s).'],
  memoryHigh: ['Memory full', 'The machine’s memory above the threshold.'],
  outputDropped: ['Output drops frames', 'An output dropped this many pictures in 10 s.'],
  colorSpace: ['Colour space', 'Colour space or transfer other than the one expected (an input carrying none is not wrong).'],
  illegalLevels: ['Illegal levels', 'Pixels out of the broadcast range (EBU R103: luma 16–235, chroma 16–240 in 8 bits) above the threshold, measured before any conversion.'],
  fullRange: ['Full range', 'The picture is coded in full range (PC) instead of limited (TV).'],
  hdrMetadata: ['HDR metadata missing', 'HDR10 (PQ) without its static metadata (ST 2086, MaxCLL / MaxFALL).'],
  noColorInfo: ['Colour not signalled', 'The stream carries no colour space, primaries nor transfer.'],
};

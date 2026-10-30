// Errors as the server writes them (English, technical) → a short text in
// the interface's language. The raw text stays available for a tooltip.
import { t } from '../i18n';

/** Why a connection failed, from the io error at the end of the raw text. */
function cause(raw: string): string | null {
  const r = raw.toLowerCase();
  if (r.includes('connection refused')) return t('connection refused');
  if (r.includes('timed out') || r.includes('timeout')) return t('timed out');
  if (r.includes('certificate') || r.includes('tls')) return t('certificate refused');
  if (r.includes('connection reset')) return t('connection reset');
  return null;
}

const unknownHost = (raw: string) => /dns error|failed to lookup|name or service not known|no such host/i.test(raw);

/** A short text for a raw server error (the raw text when it is not known). */
export function friendlyError(raw: string | null | undefined): string {
  if (!raw) return '';
  const s = raw.trim();
  let m: RegExpMatchArray | null;
  if (/^cannot reach the address/i.test(s)) {
    if (unknownHost(s)) return t('Unknown address');
    const why = cause(s);
    return why ? t('Address unreachable ({why})', { why }) : t('Address unreachable');
  }
  if (/^cannot reach the platform/i.test(s)) {
    const why = cause(s);
    return why ? t('Platform unreachable ({why})', { why }) : t('Platform unreachable');
  }
  if ((m = s.match(/^cannot reach (\w+)/i))) return t('{name} unreachable', { name: m[1] });
  if (/^cannot read the answer/i.test(s)) return t('Unreadable answer');
  if (/^(not JSON|invalid JSON)/i.test(s)) return t('The answer is not valid JSON');
  if (/^not XML/i.test(s)) return t('The answer is not valid XML');
  if ((m = s.match(/^the server answered (.+)$/i))) return t('The server answered {status}', { status: m[1] });
  if ((m = s.match(/^the platform answered (.+)$/i))) return t('The platform answered {status}', { status: m[1] });
  if ((m = s.match(/^cannot (?:listen on|bind) port (\d+).*address already in use/i))) return t('Port {port} already in use', { port: m[1] });
  if (/^no API key/i.test(s)) return t('No API key');
  if (/^no (?:access|bearer|page access) token/i.test(s)) return t('No access token');
  if (/Blackmagic Desktop Video is not installed/i.test(s)) return t('Blackmagic driver missing');
  if (/NDI runtime found but not loadable/i.test(s)) return t('NDI runtime unreadable');
  if (/NDI runtime not found/i.test(s)) return t('NDI runtime missing');
  if (/NDI runtime refused to start/i.test(s)) return t('NDI runtime refused to start');
  return s;
}

/** The short text, and the raw one as a tooltip when they differ. */
export function errorProps(raw: string | null | undefined): { text: string; title?: string } {
  const text = friendlyError(raw);
  return { text, title: raw && raw.trim() !== text ? raw.trim() : undefined };
}

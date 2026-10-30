import { ChevronDown, ChevronRight, Lock } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { t } from '../i18n';

/* Minimal OpenAPI 3.1 reader for /api/openapi.json (no external viewer:
   servers are often offline). */

type Schema = { $ref?: string; type?: string | string[]; properties?: Record<string, Schema>; items?: Schema; enum?: string[]; description?: string; example?: unknown; additionalProperties?: Schema | boolean; oneOf?: Schema[]; required?: string[]; format?: string; deprecated?: boolean };
type Param = { name: string; in: string; required?: boolean; description?: string; schema?: Schema };
type Op = { tags?: string[]; summary?: string; description?: string; parameters?: Param[]; requestBody?: { content: Record<string, { schema: Schema }> }; responses: Record<string, { description: string; content?: Record<string, { schema: Schema }> }>; security?: unknown[] };
export type Spec = { info: { title: string; version: string; description: string }; tags: { name: string }[]; paths: Record<string, Record<string, Op | Param[]>>; components: { schemas: Record<string, Schema> } };

const METHODS = ['get', 'post', 'put', 'delete'];

function typeName(s: Schema | undefined, spec: Spec): string {
  if (!s) return '';
  if (s.$ref) return s.$ref.split('/').pop()!;
  if (s.oneOf) return s.oneOf.map((x) => typeName(x, spec)).join(' | ');
  const t = Array.isArray(s.type) ? s.type.join(' | ') : (s.type ?? 'object');
  if (t === 'array') return `${typeName(s.items, spec)}[]`;
  if (s.enum) return s.enum.map((e) => `"${e}"`).join(' | ');
  return s.format ? `${t} (${s.format})` : t;
}

function SchemaView({ schema, spec, depth = 0 }: { schema: Schema; spec: Spec; depth?: number }) {
  const s = schema.$ref ? spec.components.schemas[schema.$ref.split('/').pop()!] : schema;
  if (!s) return null;
  const props = s.properties ?? (s.type === 'array' && s.items ? undefined : undefined);
  return (
    <div className="schema">
      {schema.$ref && <span className="schema-name">{schema.$ref.split('/').pop()}</span>}
      {s.description && <p className="muted small">{s.description}</p>}
      {props && (
        <table>
          <tbody>
            {Object.entries(props).map(([k, v]) => (
              <tr key={k}>
                <td>
                  <code>{k}</code>
                  {s.required?.includes(k) && <span className="req">*</span>}
                </td>
                <td className="mono">{typeName(v, spec)}</td>
                <td className="muted">
                  {v.deprecated && <em>deprecated. </em>}
                  {v.description}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {s.type === 'array' && s.items && depth < 2 && <SchemaView schema={s.items} spec={spec} depth={depth + 1} />}
      {s.example !== undefined && <pre className="example">{JSON.stringify(s.example, null, 2)}</pre>}
    </div>
  );
}

function Operation({ method, path, op, spec }: { method: string; path: string; op: Op; spec: Spec }) {
  const [open, setOpen] = useState(false);
  const body = op.requestBody && Object.entries(op.requestBody.content)[0];
  return (
    <div className={`api-op ${open ? 'open' : ''}`}>
      <button className="api-op-head" onClick={() => setOpen(!open)}>
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <span className={`method m-${method}`}>{method.toUpperCase()}</span>
        <code className="api-path">{path}</code>
        <span className="api-summary">{op.summary}</span>
        {op.security && op.security.length === 0 ? <span className="badge">{t('public')}</span> : <Lock size={12} className="muted" />}
      </button>
      {open && (
        <div className="api-op-body">
          {op.description && <p className="api-desc">{op.description}</p>}
          {!!op.parameters?.length && (
            <>
              <h4>{t('Parameters')}</h4>
              <table>
                <tbody>
                  {op.parameters.map((p) => (
                    <tr key={p.in + p.name}>
                      <td>
                        <code>{p.name}</code>
                        {p.required && <span className="req">*</span>}
                      </td>
                      <td className="muted">{p.in}</td>
                      <td className="mono">{typeName(p.schema, spec)}</td>
                      <td className="muted">{p.description}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
          {body && (
            <>
              <h4>
                {t('Body')} <span className="muted">{body[0]}</span>
              </h4>
              <SchemaView schema={body[1].schema} spec={spec} />
            </>
          )}
          <h4>{t('Responses')}</h4>
          {Object.entries(op.responses).map(([code, r]) => {
            const c = r.content && Object.entries(r.content)[0];
            return (
              <div key={code} className="api-resp">
                <span className={`code c${code[0]}`}>{code}</span> {r.description} {c && <span className="mono muted">{typeName(c[1].schema, spec)}</span>}
                {c && c[1].schema.$ref && <SchemaView schema={c[1].schema} spec={spec} />}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

export type ApiOp = { method: string; path: string; op: Op };

/** The API description, with its operations by tag. */
export function useApiSpec() {
  const [spec, setSpec] = useState<Spec | null>(null);
  useEffect(() => {
    fetch('/api/openapi.json')
      .then((r) => r.json())
      .then(setSpec)
      .catch(() => {});
  }, []);
  const byTag = useMemo(() => {
    const m = new Map<string, ApiOp[]>();
    if (!spec) return m;
    for (const [path, item] of Object.entries(spec.paths)) {
      for (const method of METHODS) {
        const op = item[method] as Op | undefined;
        if (!op) continue;
        const params = [...((item.parameters as Param[]) ?? []), ...(op.parameters ?? [])];
        const tag = op.tags?.[0] ?? 'Other';
        m.set(tag, [...(m.get(tag) ?? []), { method, path, op: { ...op, parameters: params } }]);
      }
    }
    return m;
  }, [spec]);
  return { spec, byTag };
}

const md = (para: string) =>
  para
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>');

/** The introduction of the API (authentication, conventions). */
export function ApiOverview({ spec }: { spec: Spec }) {
  return (
    <div className="api-docs">
      <div className="api-intro">
        {spec.info.description.split('\n\n').map((para, i) => (
          <p key={i} dangerouslySetInnerHTML={{ __html: md(para) }} />
        ))}
      </div>
    </div>
  );
}

/** The operations of one tag. */
export function ApiTag({ spec, name, ops }: { spec: Spec; name?: string; ops: ApiOp[] }) {
  return (
    <section className="api-tag api-docs">
      {name && <h2>{name}</h2>}
      {ops.map((o) => (
        <Operation key={o.method + o.path} {...o} spec={spec} />
      ))}
    </section>
  );
}

/** Minimal history router. */
export type ProjectPage = 'designer' | 'operator' | 'social' | 'data' | 'media';

export type Route =
  | { page: 'projects' }
  | { page: 'users' }
  | { page: 'media' }
  | { page: 'outputs' }
  | { page: 'inputs' }
  | { page: 'hardware' }
  | { page: 'performance' }
  | { page: 'health' }
  | { page: 'asrun' }
  | { page: 'data' }
  | { page: 'settings' }
  | { page: 'api' }
  | { page: 'mos' }
  | {
      page: 'project';
      projectId: string;
      sub: ProjectPage;
      channelId: string | null;
    }
  // Pre-0.10 links, resolved to the project pages.
  | { page: 'legacyDesigner'; projectId: string | null }
  | { page: 'legacyOperator'; channelId: string | null }
  | { page: 'mosPlugin' }
  | { page: 'embedPreview' }
  | { page: 'output'; channelId: string | null; rendition: string | null };

const SUBS: ProjectPage[] = ['designer', 'operator', 'social', 'data', 'media'];

export function parseRoute(path = location.pathname): Route {
  const [first, second, third, fourth] = path.split('/').filter(Boolean).map(decodeURIComponent);
  switch (first) {
    case 'project':
      if (!second) return { page: 'projects' };
      return {
        page: 'project',
        projectId: second,
        sub: SUBS.includes(third as ProjectPage) ? (third as ProjectPage) : 'designer',
        channelId: fourth ?? null,
      };
    case 'designer':
      return { page: 'legacyDesigner', projectId: second ?? null };
    case 'operator':
      return { page: 'legacyOperator', channelId: second ?? null };
    case 'settings':
      return { page: 'settings' };
    case 'users':
      return { page: 'users' };
    case 'media':
      return { page: 'media' };
    case 'outputs':
      return { page: 'outputs' };
    case 'inputs':
      return { page: 'inputs' };
    case 'hardware':
      return { page: 'hardware' };
    case 'performance':
      return { page: 'performance' };
    case 'health':
      return { page: 'health' };
    case 'asrun':
      return { page: 'asrun' };
    case 'data':
      return { page: 'data' };
    case 'mos':
      return second === 'plugin' ? { page: 'mosPlugin' } : { page: 'mos' };
    case 'embed':
      return second === 'preview' ? { page: 'embedPreview' } : { page: 'projects' };
    case 'api-docs':
      return { page: 'api' };
    case 'output':
      return {
        page: 'output',
        channelId: second ?? null,
        rendition: third ?? null,
      };
    default:
      return { page: 'projects' };
  }
}

/** URL of a project page (`channelId`: the operated channel). */
export function projectPath(projectId: string, sub: ProjectPage = 'designer', channelId?: string | null) {
  return `/project/${encodeURIComponent(projectId)}/${sub}${channelId ? `/${encodeURIComponent(channelId)}` : ''}`;
}

/** Browser tab of a project: opening it again focuses the same tab. */
export const projectTab = (projectId: string) => `pulse-project-${projectId}`;

function go(path: string, replace: boolean) {
  if (replace) history.replaceState(null, '', path);
  else history.pushState(null, '', path);
  navigating = true;
  dispatchEvent(new PopStateEvent('popstate'));
  navigating = false;
}

export function navigate(path: string, replace = false) {
  if (path === location.pathname) return;
  const g = guard;
  if (g?.leaves(path)) {
    void g.ask().then((ok) => ok && go(path, replace));
    return;
  }
  go(path, replace);
}

/**
 * Asks before leaving a page (unpublished changes…): `leaves(path)` tells
 * whether going there leaves it, `ask()` resolves true to go anyway. Also
 * covers the Back button and, with the browser's own question, closing or
 * reloading the tab. Returns the function that removes it.
 */
export function setLeaveGuard(leaves: (path: string) => boolean, ask: () => Promise<boolean>) {
  const g: Guard = { leaves, ask, at: location.pathname };
  guard = g;
  const unload = (e: BeforeUnloadEvent) => {
    e.preventDefault();
    e.returnValue = '';
  };
  addEventListener('beforeunload', unload);
  return () => {
    removeEventListener('beforeunload', unload);
    if (guard === g) guard = null;
  };
}

type Guard = { leaves: (path: string) => boolean; ask: () => Promise<boolean>; at: string };
let guard: Guard | null = null;
let navigating = false;

// Back / forward: come back first, then go if confirmed. Registered before
// the app's own listener, so the page does not change meanwhile.
addEventListener('popstate', (e) => {
  const g = guard;
  if (navigating || !g || !g.leaves(location.pathname)) return;
  e.stopImmediatePropagation();
  const to = location.pathname;
  history.pushState(null, '', g.at);
  void g.ask().then((ok) => ok && go(to, false));
});

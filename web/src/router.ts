/** Minimal history router. */
export type Route =
  | { page: 'projects' }
  | { page: 'users' }
  | { page: 'media' }
  | { page: 'outputs' }
  | { page: 'inputs' }
  | { page: 'tally' }
  | { page: 'hardware' }
  | { page: 'performance' }
  | { page: 'health' }
  | { page: 'settings' }
  | { page: 'api' }
  | { page: 'project'; projectId: string }
  | { page: 'output'; channelId: string | null };

export function parseRoute(path = location.pathname): Route {
  const [first, second] = path.split('/').filter(Boolean).map(decodeURIComponent);
  switch (first) {
    case 'project':
      if (!second) return { page: 'projects' };
      return { page: 'project', projectId: second };
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
    case 'tally':
      return { page: 'tally' };
    case 'hardware':
      return { page: 'hardware' };
    case 'performance':
      return { page: 'performance' };
    case 'health':
      return { page: 'health' };
    case 'api-docs':
      return { page: 'api' };
    case 'output':
      return { page: 'output', channelId: second ?? null };
    default:
      return { page: 'projects' };
  }
}

/** URL of a multiview's editor. */
export function projectPath(projectId: string) {
  return `/project/${encodeURIComponent(projectId)}`;
}

function go(path: string, replace: boolean) {
  if (replace) history.replaceState(null, '', path);
  else history.pushState(null, '', path);
  dispatchEvent(new PopStateEvent('popstate'));
}

export function navigate(path: string, replace = false) {
  if (path === location.pathname) return;
  go(path, replace);
}


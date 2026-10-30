import { ArrowRightToLine, Braces, CircuitBoard, MessagesSquare, ClipboardList, Database, FolderOpen, Gauge, HeartPulse, Library as LibraryIcon, MonitorPlay, Newspaper, PenTool, Radio, Server, Settings as SettingsIcon, TriangleAlert, Users as UsersIcon } from 'lucide-react';
import { needsLook, useHealth, useHealthStore } from './perf/health';
import { Brand } from './ui/Brand';
import { DialogHost } from './ui/dialogs';
import { lazy, Suspense, useEffect, useState } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { Output } from './output/Output';
import { t, tc, useLang } from './i18n';
import { Login } from './pages/Login';
import { Projects } from './pages/Projects';
import { navigate, parseRoute, projectPath, type ProjectPage, type Route } from './router';
import { can, useApp } from './store';
import { StatusPill } from './ui/StatusPill';
import { UserMenu } from './ui/UserMenu';
import { NavGroup, type NavEntry } from './ui/NavMenu';
import { ShortcutsOverlay } from './operator/ShortcutsOverlay';
import { loadDisplayPrefs, useDisplayPrefs } from './displayPrefs';

// The heavy pages load when opened: the project list and the outputs do not carry the Designer.
const Designer = lazy(() => import('./designer/Designer').then((m) => ({ default: m.Designer })));
const Operator = lazy(() => import('./operator/Operator').then((m) => ({ default: m.Operator })));
const ApiPage = lazy(() => import('./pages/ApiPage').then((m) => ({ default: m.ApiPage })));
const MosPage = lazy(() => import('./mos/MosPage').then((m) => ({ default: m.MosPage })));
const MosPlugin = lazy(() => import('./mos/Plugin').then((m) => ({ default: m.MosPlugin })));
const EmbedPreview = lazy(() => import('./embed/Preview').then((m) => ({ default: m.EmbedPreview })));
const MediaLibrary = lazy(() => import('./pages/Media').then((m) => ({ default: m.MediaLibrary })));
const Outputs = lazy(() => import('./outputs/Outputs').then((m) => ({ default: m.Outputs })));
const Performance = lazy(() => import('./perf/Performance').then((m) => ({ default: m.Performance })));
const Health = lazy(() => import('./perf/Health').then((m) => ({ default: m.Health })));
const AsRun = lazy(() => import('./pages/AsRun').then((m) => ({ default: m.AsRun })));
const Hardware = lazy(() => import('./hardware/Hardware').then((m) => ({ default: m.Hardware })));
const Users = lazy(() => import('./pages/Users').then((m) => ({ default: m.Users })));
const DataPage = lazy(() => import('./data/DataPage').then((m) => ({ default: m.DataPage })));
const Settings = lazy(() => import('./settings/Settings').then((m) => ({ default: m.Settings })));
const Social = lazy(() => import('./social/Social').then((m) => ({ default: m.Social })));

const LAST_PROJECT = 'pulse.lastProject';
const LAST_CHANNEL = 'pulse.lastChannel';
const remember = (k: string, v: string) => {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* private mode */
  }
};
const recall = (k: string) => {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
};

function useRoute(): Route {
  const [route, setRoute] = useState(parseRoute);
  useEffect(() => {
    const onPop = () => setRoute(parseRoute());
    addEventListener('popstate', onPop);
    return () => removeEventListener('popstate', onPop);
  }, []);
  return route;
}

/** While a page's code loads. */
const pageLoading = () => <div className="loading">{t('Loading…')}</div>;

export function App() {
  const route = useRoute();
  if (route.page === 'mosPlugin')
    return (
      <Suspense fallback={null}>
        <MosPlugin />
      </Suspense>
    );
  if (route.page === 'embedPreview')
    return (
      <Suspense fallback={null}>
        <EmbedPreview />
      </Suspense>
    );
  if (route.page === 'output') return <Output channelId={route.channelId ?? 'main'} rendition={route.rendition} />;
  return <Studio route={route} />;
}

function Studio({ route }: { route: Exclude<Route, { page: 'output' | 'mosPlugin' | 'embedPreview' }> }) {
  // Not the channels' live state: the pages that show it read it themselves.
  const { auth, init, connect, projects, channels } = useApp(useShallow((s) => ({ auth: s.auth, init: s.init, connect: s.connect, projects: s.projects, channels: s.channels })));
  const user = auth?.user;
  // Every text is drawn again in the new language.
  const lang = useLang((s) => s.lang);

  useEffect(() => void init(), [init]);
  useEffect(() => {
    if (user) connect();
  }, [user, connect]);
  // The user's display choices before the pages are drawn.
  const prefsOf = useDisplayPrefs((s) => s.user);
  useEffect(() => {
    if (user) void loadDisplayPrefs(user.id);
  }, [user]);

  // Pre-0.10 links (/designer/…, /operator/…) and pages a role cannot see.
  useEffect(() => {
    if (!user) return;
    if ((route.page === 'users' || route.page === 'outputs' || route.page === 'inputs' || route.page === 'hardware' || route.page === 'performance' || route.page === 'health' || route.page === 'mos') && !can.admin(user)) return navigate('/projects', true);
    if (route.page === 'legacyDesigner') {
      const id = route.projectId ?? recall(LAST_PROJECT) ?? projects[0]?.id;
      if (id) navigate(projectPath(id, 'designer'), true);
      else if (projects.length || useApp.getState().connected) navigate('/projects', true);
    }
    if (route.page === 'legacyOperator') {
      if (!channels.length) return;
      const ch = channels.find((c) => c.id === route.channelId);
      navigate(ch?.projectId ? projectPath(ch.projectId, 'operator', ch.id) : '/projects', true);
    }
  }, [route, user, projects, channels]);

  if (!auth) return <div className="loading">{t('Connecting…')}</div>;
  if (!user) return <Login key={lang} />;
  if (prefsOf !== user.id) return <div className="loading">{t('Connecting…')}</div>;

  return (
    <div className="app" key={lang}>
      {route.page === 'project' ? <ProjectShell route={route} /> : route.page === 'legacyDesigner' || route.page === 'legacyOperator' ? <div className="loading">{t('Opening…')}</div> : <Home route={route} />}
      {/* Not over a project's own bars (the Operator's on-air buttons). */}
      {can.admin(user) && route.page !== 'project' && <CrashBanner />}
      <DialogHost />
      <ShortcutsOverlay />
      <Toasts quiet={route.page === 'project' && (route.sub === 'designer' || route.sub === 'operator')} />
    </div>
  );
}

/** `quiet`: errors only (the Designer and the Operator show what an action did on the stage). */
function Toasts({ quiet }: { quiet: boolean }) {
  const toasts = useApp((s) => s.toasts);
  return (
    <div className="toasts">
      {toasts.filter((x) => !quiet || x.kind === 'error').map((t) => (
        <div key={t.id} className={`toast ${t.kind}`}>
          {t.message}
        </div>
      ))}
    </div>
  );
}

/** Projects, users and server settings. A project opens in its own browser tab. */
function Home({ route }: { route: Extract<Route, { page: 'projects' | 'media' | 'data' | 'outputs' | 'inputs' | 'hardware' | 'performance' | 'health' | 'asrun' | 'mos' | 'users' | 'api' | 'settings' }> }) {
  const user = useApp((s) => s.auth?.user);
  useEffect(() => {
    document.title = 'Strata Graphics';
  }, []);
  const tab = (page: Route['page'], path: string, icon: React.ReactNode, label: string) => (
    <button className={route.page === page ? 'on' : ''} aria-current={route.page === page ? 'page' : undefined} onClick={() => navigate(path)}>
      {icon} <span className="tab-label">{label}</span>
    </button>
  );
  const entry = (id: Route['page'], path: string, icon: React.ReactNode, label: string): NavEntry => ({ id, path, icon, label });
  const admin = can.admin(user);
  const health = useHealthStore((x) => x.health);
  const broadcast = admin ? [entry('outputs', '/outputs', <MonitorPlay size={14} />, t('Output streams')), entry('inputs', '/inputs', <ArrowRightToLine size={14} />, t('Video inputs')), entry('mos', '/mos', <Newspaper size={14} />, 'MOS')] : [];
  const system = [
    ...(admin ? [entry('hardware', '/hardware', <CircuitBoard size={14} />, t('Hardware')), entry('performance', '/performance', <Gauge size={14} />, t('Performance')), entry('health', '/health', <HeartPulse size={14} />, t('Health'))] : []),
    entry('asrun', '/asrun', <ClipboardList size={14} />, tc('tab', 'As-run')),
    ...(admin ? [entry('users', '/users', <UsersIcon size={14} />, t('Users'))] : []),
    entry('api', '/api-docs', <Braces size={14} />, 'API'),
    entry('settings', '/settings', <SettingsIcon size={14} />, t('Settings')),
  ];
  return (
    <>
      <nav className="topbar">
        <Brand />
        <div className="tabs home-tabs ux1-home-tabs">
          {tab('projects', '/projects', <FolderOpen size={14} />, t('Projects'))}
          {tab('media', '/media', <LibraryIcon size={14} />, t('Library'))}
          {can.design(user) && tab('data', '/data', <Database size={14} />, t('Data'))}
          <NavGroup icon={<Radio size={14} />} label={tc('menu', 'Broadcast')} entries={broadcast} current={route.page} />
          <NavGroup icon={<Server size={14} />} label={t('System')} entries={system} current={route.page} dot={admin && needsLook(health)} />
        </div>
        <div className="spacer" />
        <StatusPill />
        <UserMenu named />
      </nav>
      <div className="content">
        <Suspense fallback={pageLoading()}>
          {route.page === 'projects' && <Projects />}
          {route.page === 'media' && <MediaLibrary />}
          {route.page === 'data' && <DataPage />}
          {(route.page === 'outputs' || route.page === 'inputs') && <Outputs mode={route.page} />}
          {route.page === 'hardware' && <Hardware />}
          {route.page === 'performance' && <Performance />}
          {route.page === 'health' && <Health />}
          {route.page === 'asrun' && <AsRun />}
          {route.page === 'settings' && <Settings />}
          {route.page === 'mos' && <MosPage />}
          {route.page === 'users' && <Users />}
          {route.page === 'api' && <ApiPage />}
        </Suspense>
      </div>
    </>
  );
}

/** The pages of one project: designer, operator, data and project settings. */
function ProjectShell({ route }: { route: Extract<Route, { page: 'project' }> }) {
  // Not `live`: the shell is not drawn again on each frame of the channels.
  const { auth, connected, projects, channels, project, openProject } = useApp(
    useShallow((s) => ({ auth: s.auth, connected: s.connected, projects: s.projects, channels: s.channels, project: s.project, openProject: s.openProject })),
  );
  const user = auth!.user!;
  const { projectId, sub } = route;
  const summary = projects.find((p) => p.id === projectId);
  const known = !!summary || !connected || !projects.length;

  useEffect(() => {
    remember(LAST_PROJECT, projectId);
    if (useApp.getState().projectId !== projectId) openProject(projectId);
  }, [projectId, openProject]);
  useEffect(() => {
    document.title = summary ? `${summary.name} · Strata Graphics` : 'Strata Graphics';
  }, [summary?.name]);

  // Pages a role cannot see.
  const allowed = (s: ProjectPage) => (s === 'operator' || s === 'social' || s === 'media' ? can.operate(user) : can.design(user));
  useEffect(() => {
    if (!allowed(sub)) navigate(projectPath(projectId, can.design(user) ? 'designer' : 'operator'), true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sub, projectId, user]);

  // Operator: the channel playing this project (the last one used first).
  const own = channels.filter((c) => c.projectId === projectId);
  const operated = sub === 'operator' ? (channels.find((c) => c.id === route.channelId) ?? null) : null;
  useEffect(() => {
    if (sub !== 'operator' || route.channelId || !own.length) return;
    const last = recall(LAST_CHANNEL);
    navigate(projectPath(projectId, 'operator', (own.find((c) => c.id === last) ?? own[0]).id), true);
  }, [sub, route.channelId, own.length, projectId]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!operated) return;
    remember(LAST_CHANNEL, operated.id);
    useApp.getState().selectChannel(operated.id);
  }, [operated?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const tab = (s: ProjectPage, icon: React.ReactNode, label: string) =>
    allowed(s) && (
      <button className={sub === s ? 'on' : ''} title={label} onClick={() => navigate(projectPath(projectId, s, s === 'operator' ? operated?.id : null))}>
        {icon} <span className="tab-label">{label}</span>
      </button>
    );
  const ready = project && project.id === projectId;
  const projectName = summary?.name ?? project?.name ?? '…';

  let page: React.ReactNode;
  if (!known) page = <div className="loading">{t('This project does not exist or you cannot access it.')}</div>;
  else if (!ready) page = <div className="loading">{t('Opening the project…')}</div>;
  else if (sub === 'designer') page = <Designer key={project.id} />;
  else if (sub === 'social') page = <Social key={projectId} projectId={projectId} />;
  else if (sub === 'data') page = <DataPage key={projectId} projectId={projectId} />;
  else if (sub === 'media') page = <MediaLibrary key={projectId} projectId={projectId} />;
  else {
    // Without an output of this project, the operator still prepares and previews.
    const own = operated && operated.projectId === projectId ? operated.id : null;
    page = <Operator key={own ?? 'none'} channelId={own} />;
  }

  return (
    <>
      <nav className="topbar">
        <Brand />
        <span className="topbar-sep" aria-hidden />
        <span className="project-title" title={summary?.description ? `${projectName}\n${summary.description}` : projectName}>
          <small>{t('Project')}</small>
          <span>{projectName}</span>
        </span>
        <div className="tabs project-tabs">
          {tab('designer', <PenTool size={14} />, t('Designer'))}
          {tab('operator', <Radio size={14} />, t('Operator'))}
          {tab('social', <MessagesSquare size={14} />, t('Social'))}
          {tab('data', <Database size={14} />, t('Data'))}
          {tab('media', <LibraryIcon size={14} />, t('Library'))}
        </div>
        <div className="spacer" />
        {/* Filled by the page (the Designer's undo, redo and history). */}
        <div id="topbar-slot" className="topbar-slot" />
        {/* The project's own tab: the Performance page opens in another one. */}
        <StatusPill newTab />
        <UserMenu />
      </nav>
      <div className="content">
        <Suspense fallback={pageLoading()}>{page}</Suspense>
      </div>
    </>
  );
}

/** After a crash, for admins: said once, until looked at (System › Health). */
function CrashBanner() {
  const health = useHealth(true);
  const seen = useHealthStore((s) => s.seen);
  if (!health?.unseenCrash || !health.lastCrash) return null;
  return (
    <div className="crash-banner" role="alert">
      <TriangleAlert size={15} />
      <span>{t('Strata stopped unexpectedly on {when} and started again.', { when: new Date(health.lastCrash).toLocaleString(useLang.getState().lang, { dateStyle: 'short', timeStyle: 'short' }) })}</span>
      <button className="btn small" onClick={() => (void seen(), navigate('/health'))}>
        {t('See')}
      </button>
      <button className="btn small ghost" onClick={() => void seen()}>
        {t('OK')}
      </button>
    </div>
  );
}

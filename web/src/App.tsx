import { ArrowRightToLine, BellRing, Zap, Braces, Cable, CircuitBoard, FolderOpen, Gauge, HeartPulse, Library as LibraryIcon, MonitorPlay, Radio, Server, Settings as SettingsIcon, TriangleAlert, Users as UsersIcon } from 'lucide-react';
import { needsLook, useHealth, useHealthStore } from './perf/health';
import { Brand } from './ui/Brand';
import { DialogHost } from './ui/dialogs';
import { lazy, Suspense, useEffect, useState } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { Output } from './output/Output';
import { t, tc, useLang } from './i18n';
import { Login } from './pages/Login';
import { Projects } from './pages/Projects';
import { navigate, parseRoute, type Route } from './router';
import { can, useApp } from './store';
import { StatusPill } from './ui/StatusPill';
import { UserMenu } from './ui/UserMenu';
import { NavGroup, type NavEntry } from './ui/NavMenu';
import { loadDisplayPrefs, useDisplayPrefs } from './displayPrefs';

/** The logo, a link back to the multiviews. */
function HomeBrand() {
  return (
    <a
      className="brand-home"
      href="/projects"
      title={t('Multiviews')}
      onClick={(e) => {
        if (e.ctrlKey || e.metaKey || e.shiftKey || e.button !== 0) return;
        e.preventDefault();
        navigate('/projects');
      }}
    >
      <Brand />
    </a>
  );
}

// The heavy pages load when opened: the project list and the outputs do not carry the Designer.
const Designer = lazy(() => import('./designer/Designer').then((m) => ({ default: m.Designer })));
const ApiPage = lazy(() => import('./pages/ApiPage').then((m) => ({ default: m.ApiPage })));
const MediaLibrary = lazy(() => import('./pages/Media').then((m) => ({ default: m.MediaLibrary })));
const TallyPage = lazy(() => import('./tally/TallyPage').then((m) => ({ default: m.TallyPage })));
const AlarmsPage = lazy(() => import('./alarms/AlarmsPage').then((m) => ({ default: m.AlarmsPage })));
const SalvosPage = lazy(() => import('./salvos/Salvos').then((m) => ({ default: m.SalvosPage })));
const Outputs = lazy(() => import('./outputs/Outputs').then((m) => ({ default: m.Outputs })));
const Performance = lazy(() => import('./perf/Performance').then((m) => ({ default: m.Performance })));
const Health = lazy(() => import('./perf/Health').then((m) => ({ default: m.Health })));
const Hardware = lazy(() => import('./hardware/Hardware').then((m) => ({ default: m.Hardware })));
const Users = lazy(() => import('./pages/Users').then((m) => ({ default: m.Users })));
const Settings = lazy(() => import('./settings/Settings').then((m) => ({ default: m.Settings })));

const LAST_PROJECT = 'strata-mv.lastProject';
const remember = (k: string, v: string) => {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* private mode */
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
  if (route.page === 'output') return <Output channelId={route.channelId ?? 'main'} />;
  return <Studio route={route} />;
}

function Studio({ route }: { route: Exclude<Route, { page: 'output' }> }) {
  // Not the channels' live state: the pages that show it read it themselves.
  const { auth, init, connect } = useApp(useShallow((s) => ({ auth: s.auth, init: s.init, connect: s.connect })));
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

  // Pages a role cannot see.
  useEffect(() => {
    if (!user) return;
    if ((route.page === 'users' || route.page === 'outputs' || route.page === 'inputs' || route.page === 'tally' || route.page === 'hardware' || route.page === 'performance' || route.page === 'health') && !can.admin(user)) return navigate('/projects', true);
  }, [route, user]);

  if (!auth) return <div className="loading">{t('Connecting…')}</div>;
  if (!user) return <Login key={lang} />;
  if (prefsOf !== user.id) return <div className="loading">{t('Connecting…')}</div>;

  return (
    <div className="app" key={lang}>
      {route.page === 'project' ? <ProjectShell route={route} /> : <Home route={route} />}
      {/* Not over a project's own bars. */}
      {can.admin(user) && route.page !== 'project' && <CrashBanner />}
      <DialogHost />
      <Toasts quiet={route.page === 'project'} />
    </div>
  );
}

/** `quiet`: errors only (the Designer shows what an action did on the stage). */
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
function Home({ route }: { route: Extract<Route, { page: 'projects' | 'media' | 'outputs' | 'inputs' | 'tally' | 'alarms' | 'salvos' | 'hardware' | 'performance' | 'health' | 'users' | 'api' | 'settings' }> }) {
  const user = useApp((s) => s.auth?.user);
  useEffect(() => {
    document.title = 'Strata Multiview';
  }, []);
  const tab = (page: Route['page'], path: string, icon: React.ReactNode, label: string) => (
    <button className={route.page === page ? 'on' : ''} aria-current={route.page === page ? 'page' : undefined} onClick={() => navigate(path)}>
      {icon} <span className="tab-label">{label}</span>
    </button>
  );
  const entry = (id: Route['page'], path: string, icon: React.ReactNode, label: string): NavEntry => ({ id, path, icon, label });
  const admin = can.admin(user);
  const health = useHealthStore((x) => x.health);
  const broadcast = [
    ...(admin ? [entry('outputs', '/outputs', <MonitorPlay size={14} />, t('Output streams')), entry('inputs', '/inputs', <ArrowRightToLine size={14} />, t('Video inputs')), entry('tally', '/tally', <Cable size={14} />, t('Tally & UMD'))] : []),
    entry('alarms', '/alarms', <BellRing size={14} />, t('Alarms')),
    entry('salvos', '/salvos', <Zap size={14} />, t('Salvos')),
  ];
  const system = [
    ...(admin ? [entry('hardware', '/hardware', <CircuitBoard size={14} />, t('Hardware')), entry('performance', '/performance', <Gauge size={14} />, t('Performance')), entry('health', '/health', <HeartPulse size={14} />, t('Health'))] : []),
    ...(admin ? [entry('users', '/users', <UsersIcon size={14} />, t('Users'))] : []),
    entry('api', '/api-docs', <Braces size={14} />, 'API'),
    entry('settings', '/settings', <SettingsIcon size={14} />, t('Settings')),
  ];
  return (
    <>
      <nav className="topbar">
        <HomeBrand />
        <div className="tabs home-tabs ux1-home-tabs">
          {tab('projects', '/projects', <FolderOpen size={14} />, t('Multiviews'))}
          {tab('media', '/media', <LibraryIcon size={14} />, t('Library'))}
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
          {(route.page === 'outputs' || route.page === 'inputs') && <Outputs mode={route.page} />}
          {route.page === 'tally' && <TallyPage />}
          {route.page === 'alarms' && <AlarmsPage />}
          {route.page === 'salvos' && <SalvosPage />}
          {route.page === 'hardware' && <Hardware />}
          {route.page === 'performance' && <Performance />}
          {route.page === 'health' && <Health />}
          {route.page === 'settings' && <Settings />}
          {route.page === 'users' && <Users />}
          {route.page === 'api' && <ApiPage />}
        </Suspense>
      </div>
    </>
  );
}

/** One multiview, edited in the Designer. */
function ProjectShell({ route }: { route: Extract<Route, { page: 'project' }> }) {
  // Not `live`: the shell is not drawn again on each frame of the channels.
  const { auth, connected, projects, project, openProject } = useApp(
    useShallow((s) => ({ auth: s.auth, connected: s.connected, projects: s.projects, project: s.project, openProject: s.openProject })),
  );
  const user = auth!.user!;
  const { projectId } = route;
  const summary = projects.find((p) => p.id === projectId);
  const known = !!summary || !connected || !projects.length;

  useEffect(() => {
    remember(LAST_PROJECT, projectId);
    if (useApp.getState().projectId !== projectId) openProject(projectId);
  }, [projectId, openProject]);
  useEffect(() => {
    document.title = summary ? `${summary.name} · Strata Multiview` : 'Strata Multiview';
  }, [summary?.name]);

  const ready = project && project.id === projectId;
  const projectName = summary?.name ?? project?.name ?? '…';

  let page: React.ReactNode;
  if (!known) page = <div className="loading">{t('This multiview does not exist or you cannot access it.')}</div>;
  else if (!can.design(user)) page = <div className="loading">{t('Your role cannot edit the multiviews.')}</div>;
  else if (!ready) page = <div className="loading">{t('Opening the multiview…')}</div>;
  else page = <Designer key={project.id} />;

  return (
    <>
      <nav className="topbar">
        <HomeBrand />
        <span className="topbar-sep" aria-hidden />
        <span className="project-title" title={summary?.description ? `${projectName}\n${summary.description}` : projectName}>
          <small>{t('Multiview')}</small>
          <span>{projectName}</span>
        </span>
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

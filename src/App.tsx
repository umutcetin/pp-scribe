import { LiveView } from './components/LiveView';
import { MeetingView } from './components/MeetingView';
import { SettingsPanel } from './components/SettingsPanel';
import { Sidebar } from './components/Sidebar';
import { Toasts } from './components/Toasts';
import { useBackendEvents } from './hooks/useBackendEvents';
import { useApp } from './stores/appStore';

export default function App() {
  useBackendEvents();
  const view = useApp((s) => s.view);
  const selectedId = useApp((s) => s.selectedId);

  return (
    <div className="flex h-full bg-surface-900 text-surface-200">
      <Sidebar />
      <main className="flex-1 min-w-0">
        {view === 'meeting' && selectedId ? <MeetingView id={selectedId} /> : <LiveView />}
      </main>
      <SettingsPanel />
      <Toasts />
    </div>
  );
}

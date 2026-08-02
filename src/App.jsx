import { lazy, Suspense, useState } from 'react';
import { useControlPanel } from './features/control/useControlPanel';
import { PaletteWindow } from './features/palette/PaletteWindow';
import { useRewriteSession } from './features/palette/useRewriteSession';

const ControlPanel = lazy(async () => {
  const module = await import('./features/control/ControlPanel');
  return { default: module.ControlPanel };
});

function App() {
  const [view, setView] = useState('palette');
  const rewrite = useRewriteSession(view, setView);
  const control = useControlPanel(setView, rewrite.setStatus);
  if (view === 'control' && control.settings)
    return (
      <Suspense
        fallback={
          <main className="control-panel">
            <div className="control-notice">正在加载控制面板…</div>
          </main>
        }
      >
        <ControlPanel {...control.panelProps} />
      </Suspense>
    );
  return (
    <PaletteWindow
      {...rewrite}
      onOpenControl={control.openControl}
      onCancel={rewrite.cancel}
      onBeginAnalysis={rewrite.beginAnalysis}
      onAccept={rewrite.acceptReplacement}
    />
  );
}

export default App;

import { lazy, Suspense, useState } from 'react';
import { Button, Spin, Tabs } from 'antd';
import { ArrowLeftOutlined, CloseOutlined } from '@ant-design/icons';
import { CombinationEditorPage, CombinationPanel } from './WorkCombinationPanel';
import './ControlPanel.css';

const ModelProviderPanel = lazy(async () => {
  const module = await import('./ModelProviderPanel');
  return { default: module.ModelProviderPanel };
});
const CatalogPanel = lazy(async () => {
  const module = await import('./CatalogPanel');
  return { default: module.CatalogPanel };
});
const FeatureAndShortcutPanel = lazy(async () => {
  const module = await import('./FeatureAndShortcutPanel');
  return { default: module.FeatureAndShortcutPanel };
});
const DiagnosticsPanel = lazy(async () => {
  const module = await import('./DiagnosticsPanel');
  return { default: module.DiagnosticsPanel };
});

function TabLoading() {
  return (
    <div className="control-notice">
      <Spin size="small" /> 正在加载页面…
    </div>
  );
}

export function ControlPanel({
  payload,
  busy,
  notice,
  canReturn,
  onBack,
  onClose,
  onImportAgent,
  onImportKnowledgeBases,
  onDelete,
  onSaveCombination,
  onDeleteCombination,
  onSetDefaultCombination,
  onGenerateKnowledgeBaseIndex,
  onSpecifyKnowledgeBaseIndex,
  onSaveModelProvider,
  onDeleteModelProvider,
  onSetDefaultModelProvider,
  onTestModelProvider,
  diagnostics,
  onOpenDiagnostics,
  onRefreshDiagnostics,
  onCopyDiagnostics,
  onClearDiagnostics,
  featureErrors,
  onFeatureShortcutSave,
}) {
  const [editingCombination, setEditingCombination] = useState(null);
  const data = payload.settings;
  const featureSettingsKey = JSON.stringify({
    featureToggles: data.featureToggles,
    shortcuts: data.shortcuts,
    referenceShortcut: data.referenceShortcut,
    referenceCaptureMode: data.referenceCaptureMode,
    knowledgeBaseInlineTokenLimit: data.knowledgeBaseInlineTokenLimit,
  });
  const titlebar = (
    <header className="palette-titlebar">
      <div className="palette-drag-region" data-tauri-drag-region>
        <span className="app-mark">控制面板</span>
      </div>
      <div className="titlebar-actions" data-no-window-drag>
        {canReturn && (
          <Button type="text" size="small" icon={<ArrowLeftOutlined />} onClick={onBack}>
            返回优化
          </Button>
        )}
        <Button type="text" size="small" icon={<CloseOutlined />} onClick={onClose}>
          关闭
        </Button>
      </div>
    </header>
  );
  if (editingCombination)
    return (
      <main className="control-panel">
        {titlebar}
        <div className="control-notice">
          {busy && <Spin size="small" />}
          {notice || (editingCombination.id ? '编辑工作组合' : '新建工作组合')}
        </div>
        <CombinationEditorPage
          payload={payload}
          combination={editingCombination.id ? editingCombination : null}
          busy={busy}
          onBack={() => setEditingCombination(null)}
          onSave={async (input) => {
            if (await onSaveCombination(input)) setEditingCombination(null);
          }}
        />
      </main>
    );
  return (
    <main className="control-panel">
      {titlebar}
      <div className="control-notice">
        {busy && <Spin size="small" />}
        {notice || '本地配置与资料管理'}
      </div>
      <Tabs
        size="small"
        onChange={(key) => {
          if (key === 'diagnostics') onOpenDiagnostics();
        }}
        items={[
          {
            key: 'combinations',
            label: '组合',
            children: (
              <CombinationPanel
                payload={payload}
                busy={busy}
                onOpenEditor={() =>
                  setEditingCombination({ id: '', name: '', agentId: '', knowledgeBaseIds: [] })
                }
                onEdit={setEditingCombination}
                onDelete={onDeleteCombination}
                onSetDefault={onSetDefaultCombination}
              />
            ),
          },
          {
            key: 'model',
            label: 'AI 服务',
            children: (
              <Suspense fallback={<TabLoading />}>
                <ModelProviderPanel
                  payload={payload}
                  busy={busy}
                  onSave={onSaveModelProvider}
                  onDelete={onDeleteModelProvider}
                  onSetDefault={onSetDefaultModelProvider}
                  onTest={onTestModelProvider}
                />
              </Suspense>
            ),
          },
          {
            key: 'agents',
            label: 'Agent',
            children: (
              <Suspense fallback={<TabLoading />}>
                <CatalogPanel
                  label="Agent"
                  items={payload.agents}
                  busy={busy}
                  onImport={onImportAgent}
                  onDelete={(id) => onDelete('delete_agent', id, 'Agent')}
                />
              </Suspense>
            ),
          },
          {
            key: 'knowledge',
            label: '知识库',
            children: (
              <Suspense fallback={<TabLoading />}>
                <CatalogPanel
                  label="知识库"
                  items={payload.knowledgeBases}
                  busy={busy}
                  onImport={onImportKnowledgeBases}
                  onDelete={(id) => onDelete('delete_knowledge_base', id, '知识库')}
                  onGenerateKnowledgeBaseIndex={onGenerateKnowledgeBaseIndex}
                  onSpecifyKnowledgeBaseIndex={onSpecifyKnowledgeBaseIndex}
                />
              </Suspense>
            ),
          },
          {
            key: 'features',
            label: '功能与快捷键',
            children: (
              <Suspense fallback={<TabLoading />}>
                <FeatureAndShortcutPanel
                  key={featureSettingsKey}
                  settings={data}
                  busy={busy}
                  errors={featureErrors}
                  onSave={onFeatureShortcutSave}
                />
              </Suspense>
            ),
          },
          {
            key: 'diagnostics',
            label: '诊断与日志',
            children: (
              <Suspense fallback={<TabLoading />}>
                <DiagnosticsPanel
                  diagnostics={diagnostics}
                  busy={busy}
                  onRefresh={onRefreshDiagnostics}
                  onCopy={onCopyDiagnostics}
                  onClear={onClearDiagnostics}
                />
              </Suspense>
            ),
          },
        ]}
      />
    </main>
  );
}

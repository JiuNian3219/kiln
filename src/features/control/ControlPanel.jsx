import { useState } from 'react';
import { Button, Spin, Tabs } from 'antd';
import { ArrowLeftOutlined, CloseOutlined } from '@ant-design/icons';
import { FeatureAndShortcutPanel } from './FeatureAndShortcutPanel';
import { ModelProviderPanel } from './ModelProviderPanel';
import { CombinationEditorPage, CombinationPanel } from './WorkCombinationPanel';
import { CatalogPanel } from './CatalogPanel';
import { DiagnosticsPanel } from './DiagnosticsPanel';
import './ControlPanel.css';

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
              <ModelProviderPanel
                payload={payload}
                busy={busy}
                onSave={onSaveModelProvider}
                onDelete={onDeleteModelProvider}
                onSetDefault={onSetDefaultModelProvider}
                onTest={onTestModelProvider}
              />
            ),
          },
          {
            key: 'agents',
            label: 'Agent',
            children: (
              <CatalogPanel
                label="Agent"
                items={payload.agents}
                busy={busy}
                onImport={onImportAgent}
                onDelete={(id) => onDelete('delete_agent', id, 'Agent')}
              />
            ),
          },
          {
            key: 'knowledge',
            label: '知识库',
            children: (
              <CatalogPanel
                label="知识库"
                items={payload.knowledgeBases}
                busy={busy}
                onImport={onImportKnowledgeBases}
                onDelete={(id) => onDelete('delete_knowledge_base', id, '知识库')}
                onGenerateKnowledgeBaseIndex={onGenerateKnowledgeBaseIndex}
                onSpecifyKnowledgeBaseIndex={onSpecifyKnowledgeBaseIndex}
              />
            ),
          },
          {
            key: 'features',
            label: '功能与快捷键',
            children: (
              <FeatureAndShortcutPanel
                key={featureSettingsKey}
                settings={data}
                busy={busy}
                errors={featureErrors}
                onSave={onFeatureShortcutSave}
              />
            ),
          },
          {
            key: 'diagnostics',
            label: '诊断与日志',
            children: (
              <DiagnosticsPanel
                diagnostics={diagnostics}
                busy={busy}
                onRefresh={onRefreshDiagnostics}
                onCopy={onCopyDiagnostics}
                onClear={onClearDiagnostics}
              />
            ),
          },
        ]}
      />
    </main>
  );
}

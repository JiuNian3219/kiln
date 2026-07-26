import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Button,
  Empty,
  Form,
  Input,
  List,
  Popconfirm,
  Select,
  Spin,
  Switch,
  Tabs,
  Tooltip,
  Typography,
} from 'antd';
import {
  ArrowLeftOutlined,
  CloseOutlined,
  DeleteOutlined,
  EditOutlined,
  InboxOutlined,
  PlusOutlined,
  QuestionCircleOutlined,
} from '@ant-design/icons';
import { invoke } from '../lib/tauri';

const { Text } = Typography;
const preview = (text, limit = 240) => (text.length > limit ? `${text.slice(0, limit).trimEnd()}?` : text);
export function PreviewBlock({ label, text, accent }) {
  return (
    <div className={accent ? 'preview-block accent' : 'preview-block'}>
      <label>{label}</label>
      <pre>{preview(text, 260)}</pre>
    </div>
  );
}
export function ActionBar({ children }) {
  return <footer className="action-bar">{children}</footer>;
}

const shortcutLabels = {
  'read-selection': '读取选区并生成',
  'open-control-panel': '打开控制面板',
  'quit-app': '退出程序',
};
const shortcutDefaults = {
  'read-selection': 'Ctrl+Alt+E',
  'open-control-panel': 'Ctrl+Shift+Alt+S',
  'quit-app': 'Ctrl+Alt+Q',
};
const toggleDefaults = { 'network-search': true, 'reference-context': true };
const asShortcutSettings = (settings) => ({
  featureToggles: { ...toggleDefaults, ...(settings.featureToggles || {}) },
  shortcuts: { ...shortcutDefaults, ...(settings.shortcuts || {}) },
  referenceShortcut: settings.referenceShortcut || 'Ctrl+Shift+T',
  referenceCaptureMode: settings.referenceCaptureMode === 'clipboard' ? 'clipboard' : 'selection',
});

export function FeatureAndShortcutPanel({ settings, busy, errors, onSave }) {
  const [draft, setDraft] = useState(() => asShortcutSettings(settings));
  const [waitingFor, setWaitingFor] = useState('');
  const [captureMessage, setCaptureMessage] = useState('');
  const shortcutsSuspended = useRef(false);
  const resumeGlobalShortcuts = useCallback(() => {
    if (!shortcutsSuspended.current) return;
    shortcutsSuspended.current = false;
    invoke('resume_global_shortcuts').catch(() =>
      setCaptureMessage('恢复全局快捷键失败，请重新打开控制面板。'),
    );
  }, []);
  const stopShortcutCapture = useCallback(() => {
    setWaitingFor('');
    resumeGlobalShortcuts();
  }, [resumeGlobalShortcuts]);
  useEffect(() => () => resumeGlobalShortcuts(), [resumeGlobalShortcuts]);
  const duplicates = useMemo(() => {
    const allShortcuts = { ...draft.shortcuts, 'reference-context': draft.referenceShortcut };
    return Object.entries(allShortcuts).reduce(
      (all, [key, value]) => ({
        ...all,
        [key]:
          value &&
          Object.entries(allShortcuts).some(
            ([otherKey, otherValue]) => otherKey !== key && otherValue === value,
          ),
      }),
      {},
    );
  }, [draft.shortcuts, draft.referenceShortcut]);
  const startShortcutCapture = async (key) => {
    if (waitingFor === key || busy) return;
    setCaptureMessage('');
    try {
      if (!shortcutsSuspended.current) {
        await invoke('suspend_global_shortcuts');
        shortcutsSuspended.current = true;
      }
      setWaitingFor(key);
    } catch (error) {
      setCaptureMessage(`无法进入快捷键录入：${error}`);
    }
  };
  useEffect(() => {
    if (!waitingFor) return undefined;
    const captureShortcut = (event) => {
      if (event.repeat) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      if (event.code === 'Escape') {
        stopShortcutCapture();
        return;
      }
      const key = /^Key[A-Z]$/.test(event.code)
        ? event.code.slice(3)
        : /^Digit[0-9]$/.test(event.code)
          ? event.code.slice(5)
          : '';
      if (!key) {
        setCaptureMessage('请按 Ctrl、Alt 或 Shift 加一个字母或数字；按 Esc 取消。');
        return;
      }
      if (!event.ctrlKey && !event.altKey && !event.shiftKey) {
        setCaptureMessage('快捷键必须包含 Ctrl、Alt 或 Shift。');
        return;
      }
      const chord = [event.ctrlKey && 'Ctrl', event.altKey && 'Alt', event.shiftKey && 'Shift', key]
        .filter(Boolean)
        .join('+');
      setDraft((current) =>
        waitingFor === 'reference-context'
          ? { ...current, referenceShortcut: chord }
          : { ...current, shortcuts: { ...current.shortcuts, [waitingFor]: chord } },
      );
      setCaptureMessage('');
      stopShortcutCapture();
    };
    window.addEventListener('keydown', captureShortcut, true);
    return () => window.removeEventListener('keydown', captureShortcut, true);
  }, [waitingFor, stopShortcutCapture]);
  return (
    <section className="feature-shortcut-panel">
      <div className="feature-scroll">
        <div className="feature-section">
          <Text>功能开关</Text>
          <span>开关会立即应用于之后的新会话。</span>
          <div className="feature-switches">
            <Switch
              checked={draft.featureToggles['network-search']}
              onChange={(value) =>
                setDraft((current) => ({
                  ...current,
                  featureToggles: { ...current.featureToggles, 'network-search': value },
                }))
              }
              checkedChildren="开"
              unCheckedChildren="关"
            />
            <label className="feature-label">
              联网搜索{' '}
              <Tooltip title="开启后，优化任务可直接使用联网搜索和网页读取来补充最新公开资料；关闭后不会提供联网工具。">
                <QuestionCircleOutlined />
              </Tooltip>
            </label>
            <Switch
              checked={draft.featureToggles['reference-context']}
              onChange={(value) =>
                setDraft((current) => ({
                  ...current,
                  featureToggles: { ...current.featureToggles, 'reference-context': value },
                }))
              }
              checkedChildren="开"
              unCheckedChildren="关"
            />
            <label className="feature-label">
              参考上下文{' '}
              <Tooltip title="将剪贴板内容或通过快捷键选取的文本暂存为参考资料，在下一次生成时可选择附带给 Agent。">
                <QuestionCircleOutlined />
              </Tooltip>
            </label>
          </div>
          <div className="reference-capture-settings">
            <label>参考上下文读取方式</label>
            <Select
              size="small"
              value={draft.referenceCaptureMode}
              disabled={!draft.featureToggles['reference-context']}
              options={[
                { value: 'clipboard', label: '剪贴板模式：读取当前剪贴板' },
                { value: 'selection', label: '快捷键选择模式：读取当前选区' },
              ]}
              onChange={(value) => setDraft((current) => ({ ...current, referenceCaptureMode: value }))}
            />
            {draft.referenceCaptureMode === 'selection' && (
              <div className="shortcut-field reference-shortcut-field">
                <label>读取参考上下文</label>
                <Input
                  className={waitingFor === 'reference-context' ? 'shortcut-capturing' : ''}
                  value={waitingFor === 'reference-context' ? '按下组合键…' : draft.referenceShortcut}
                  readOnly
                  disabled={!draft.featureToggles['reference-context']}
                  status={errors?.referenceShortcut ? 'error' : undefined}
                  onClick={() => startShortcutCapture('reference-context')}
                  onBlur={() => waitingFor === 'reference-context' && stopShortcutCapture()}
                />
                {errors?.referenceShortcut && (
                  <small className="shortcut-error">{errors.referenceShortcut}</small>
                )}
                {duplicates['reference-context'] && (
                  <small className="shortcut-warning">与其他操作重复；运行时最后注册的操作生效。</small>
                )}
              </div>
            )}
          </div>
        </div>
        <div className="feature-section">
          <Text>快捷键自定义</Text>
          <span>点击输入框后按下组合键。必须包含 Ctrl、Alt 或 Shift，以及一个字母或数字。</span>
          <div className="shortcut-fields">
            {Object.entries(shortcutLabels).map(([key, label]) => (
              <div className="shortcut-field" key={key}>
                <label>{label}</label>
                <Input
                  className={waitingFor === key ? 'shortcut-capturing' : ''}
                  value={waitingFor === key ? '按下组合键…' : draft.shortcuts[key]}
                  readOnly
                  status={errors?.[key] ? 'error' : undefined}
                  onClick={() => startShortcutCapture(key)}
                  onBlur={() => waitingFor === key && stopShortcutCapture()}
                />
                {errors?.[key] && <small className="shortcut-error">{errors[key]}</small>}
                {duplicates[key] && (
                  <small className="shortcut-warning">与其他操作重复；运行时最后注册的操作生效。</small>
                )}
              </div>
            ))}
          </div>
          {(waitingFor || captureMessage) && (
            <div
              className={waitingFor ? 'shortcut-capture-hint active' : 'shortcut-capture-hint'}
              aria-live="polite"
            >
              {waitingFor ? '正在录入快捷键：按下组合键，或按 Esc 取消。' : captureMessage}
            </div>
          )}
        </div>
      </div>
      <div className="feature-section storage-section">
        <Text>存储/应用</Text>
        <span>保存后立即重注册全局快捷键，并写入本机 settings.json。</span>
        <ActionBar>
          <Button type="primary" onClick={() => onSave(draft)} loading={busy}>
            保存
          </Button>
        </ActionBar>
      </div>
    </section>
  );
}

function CombinationPanel({ payload, busy, onOpenEditor, onEdit, onDelete, onSetDefault }) {
  const combinations = payload.settings.combinations || [];
  return (
    <section className="combination-panel">
      <div className="catalog-heading">
        <Text>工作组合</Text>
        <Button type="primary" size="small" icon={<PlusOutlined />} disabled={busy} onClick={onOpenEditor}>
          新建组合
        </Button>
      </div>
      <List
        size="small"
        className={combinations.length ? '' : 'combination-list-empty'}
        dataSource={combinations}
        locale={{
          emptyText: (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无组合">
              <Button
                type="primary"
                size="small"
                icon={<PlusOutlined />}
                disabled={busy}
                onClick={onOpenEditor}
              >
                新建组合
              </Button>
            </Empty>
          ),
        }}
        renderItem={(item) => (
          <List.Item
            actions={[
              <Button
                key="edit"
                type="text"
                size="small"
                icon={<EditOutlined />}
                onClick={() => onEdit(item)}
              >
                编辑
              </Button>,
              <Button
                key="default"
                size="small"
                type={payload.settings.defaultCombination === item.id ? 'primary' : 'text'}
                onClick={() => onSetDefault(item.id)}
              >
                默认
              </Button>,
              <Popconfirm
                key="delete"
                title={`删除「${item.name}」？`}
                okText="删除"
                cancelText="取消"
                onConfirm={() => onDelete(item.id)}
              >
                <Button danger type="text" size="small" icon={<DeleteOutlined />} />
              </Popconfirm>,
            ]}
          >
            <List.Item.Meta
              title={item.name}
              description={`Agent：${item.agentId} · 知识库：${item.knowledgeBaseIds.length} 个`}
            />
          </List.Item>
        )}
      />
    </section>
  );
}

function CombinationEditorPage({ payload, combination, busy, onBack, onSave }) {
  const [draft, setDraft] = useState(() => ({
    id: combination?.id || '',
    name: combination?.name || '',
    agentId: combination?.agentId || '',
    knowledgeBaseIds: combination?.knowledgeBaseIds || [],
  }));
  const agents = payload.agents.map((item) => ({ value: item.id, label: item.name }));
  const knowledgeBases = payload.knowledgeBases
    .filter((item) => item.indexStatus !== '缺少 INDEX')
    .map((item) => ({ value: item.id, label: item.name }));
  const canSave = draft.name.trim() && draft.agentId && draft.knowledgeBaseIds.length;
  return (
    <section className="combination-editor-page">
      <div className="combination-editor-heading">
        <Button type="text" size="small" icon={<ArrowLeftOutlined />} onClick={onBack} disabled={busy}>
          返回组合
        </Button>
        <div>
          <Text>{draft.id ? '编辑工作组合' : '新建工作组合'}</Text>
          <span>将一个 Agent 与一个或多个知识库固定为可复用的工作方式。</span>
        </div>
      </div>
      <Form layout="vertical" className="combination-editor-form">
        <Form.Item label="组合名称">
          <Input
            autoFocus
            value={draft.name}
            placeholder="例如：产品实现"
            onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))}
          />
        </Form.Item>
        <Form.Item label="Agent">
          <Select
            value={draft.agentId || undefined}
            options={agents}
            placeholder="选择 Agent"
            onChange={(agentId) => setDraft((current) => ({ ...current, agentId }))}
          />
        </Form.Item>
        <Form.Item label="知识库">
          <Select
            mode="multiple"
            value={draft.knowledgeBaseIds}
            options={knowledgeBases}
            placeholder="选择一个或多个知识库"
            onChange={(knowledgeBaseIds) => setDraft((current) => ({ ...current, knowledgeBaseIds }))}
          />
        </Form.Item>
      </Form>
      <ActionBar>
        <Button disabled={busy} onClick={onBack}>
          取消
        </Button>
        <Button
          type="primary"
          loading={busy}
          disabled={!canSave}
          onClick={() => onSave({ ...draft, name: draft.name.trim() })}
        >
          {draft.id ? '保存修改' : '创建组合'}
        </Button>
      </ActionBar>
    </section>
  );
}

export function ControlPanel({
  payload,
  busy,
  notice,
  canReturn,
  onBack,
  onClose,
  onChange,
  onImportAgent,
  onImportKnowledgeBases,
  onDelete,
  onSaveCombination,
  onDeleteCombination,
  onSetDefaultCombination,
  onGenerateKnowledgeBaseIndex,
  onSpecifyKnowledgeBaseIndex,
  onSave,
  onTest,
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
  const catalog = (label, items, onImportAction, deleteCommand) => (
    <>
      <div className="catalog-heading">
        <Text>{label}</Text>
        <Button size="small" icon={<InboxOutlined />} onClick={onImportAction}>
          导入
        </Button>
      </div>
      <List
        size="small"
        dataSource={items}
        locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={`暂无${label}`} /> }}
        renderItem={(item) => (
          <List.Item
            actions={[
              ...(label === '知识库'
                ? [
                    <Button
                      key="generate-index"
                      size="small"
                      onClick={() => onGenerateKnowledgeBaseIndex(item.id)}
                    >
                      AI 索引
                    </Button>,
                    <Button
                      key="manual-index"
                      size="small"
                      onClick={() => onSpecifyKnowledgeBaseIndex(item.id)}
                    >
                      指定索引
                    </Button>,
                  ]
                : []),
              <Popconfirm
                key="delete"
                title={`删除「${item.name}」？`}
                description="将删除已导入的本地目录，无法撤销。"
                okText="删除"
                cancelText="取消"
                onConfirm={() => onDelete(deleteCommand, item.id, label)}
              >
                <Button danger type="text" size="small" icon={<DeleteOutlined />} />
              </Popconfirm>,
            ]}
          >
            <List.Item.Meta title={item.name} description={item.indexStatus || '已导入本地资料库'} />
          </List.Item>
        )}
      />
    </>
  );
  const logEvents = diagnostics?.recentEvents || [];
  const logTime = (milliseconds) => new Date(milliseconds).toLocaleString('zh-CN', { hour12: false });
  const diagnosticsPanel = (
    <section className="diagnostics-panel">
      <div className="diagnostics-intro">
        <Text>本地诊断日志</Text>
        <span>
          仅保存在本机，最长保留 {diagnostics?.retentionDays ?? 14} 天；不含草稿、资料正文、检索词、目录路径或
          API Key。
        </span>
      </div>
      <div className="diagnostics-actions">
        <Button size="small" onClick={onRefreshDiagnostics} loading={busy}>
          刷新
        </Button>
        <Button size="small" onClick={onCopyDiagnostics} disabled={!diagnostics?.report}>
          复制诊断摘要
        </Button>
        <Popconfirm
          title="清除本地诊断日志？"
          description="此操作会删除当前设备上的排障记录。"
          okText="清除"
          cancelText="取消"
          onConfirm={onClearDiagnostics}
        >
          <Button size="small" danger disabled={busy}>
            清除日志
          </Button>
        </Popconfirm>
      </div>
      <div className="diagnostics-list">
        {logEvents.length ? (
          logEvents
            .slice()
            .reverse()
            .map((event, index) => (
              <article className={`diagnostic-event ${event.level}`} key={`${event.timestampMs}-${index}`}>
                <time>{logTime(event.timestampMs)}</time>
                <div>
                  <strong>{event.event}</strong>
                  <span>
                    {event.errorCode || '正常'} · {event.sessionId || '应用级事件'}
                  </span>
                </div>
              </article>
            ))
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂时没有诊断记录" />
        )}
      </div>
    </section>
  );
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
            key: 'features',
            label: '功能和快捷键',
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
            key: 'model',
            label: '模型',
            children: (
              <Form layout="vertical" className="compact-form">
                <Form.Item label="DeepSeek 模型">
                  <Input
                    size="small"
                    value={data.model}
                    onChange={(event) => onChange('model', event.target.value)}
                  />
                </Form.Item>
                <Form.Item label="DeepSeek API Key">
                  <Input.Password
                    size="small"
                    value={data.apiKey || ''}
                    placeholder={payload.apiKeyConfigured ? '已配置；留空保留当前 Key' : '尚未配置'}
                    onChange={(event) => onChange('apiKey', event.target.value)}
                  />
                </Form.Item>
                <ActionBar>
                  <Button onClick={onTest} loading={busy}>
                    测试 API
                  </Button>
                  <Button type="primary" onClick={onSave} loading={busy}>
                    保存
                  </Button>
                </ActionBar>
              </Form>
            ),
          },
          {
            key: 'agents',
            label: 'Agent',
            children: catalog('Agent', payload.agents, onImportAgent, 'delete_agent'),
          },
          {
            key: 'knowledge',
            label: '知识库',
            children: catalog(
              '知识库',
              payload.knowledgeBases,
              onImportKnowledgeBases,
              'delete_knowledge_base',
            ),
          },
          { key: 'diagnostics', label: '诊断与日志', children: diagnosticsPanel },
        ]}
      />
    </main>
  );
}

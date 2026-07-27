import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Button, Input, Select, Switch, Tooltip, Typography } from 'antd';
import { QuestionCircleOutlined } from '@ant-design/icons';
import { ActionBar } from '../../components/ActionBar';
import { invoke } from '../../lib/tauri';
import './FeatureAndShortcutPanel.css';

const { Text } = Typography;
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
              <ShortcutField
                label="读取参考上下文"
                value={draft.referenceShortcut}
                field="reference-context"
                waitingFor={waitingFor}
                disabled={!draft.featureToggles['reference-context']}
                error={errors?.referenceShortcut}
                duplicate={duplicates['reference-context']}
                onStart={startShortcutCapture}
                onStop={stopShortcutCapture}
              />
            )}
          </div>
        </div>
        <div className="feature-section">
          <Text>快捷键自定义</Text>
          <span>点击输入框后按下组合键。必须包含 Ctrl、Alt 或 Shift，以及一个字母或数字。</span>
          <div className="shortcut-fields">
            {Object.entries(shortcutLabels).map(([key, label]) => (
              <ShortcutField
                key={key}
                label={label}
                value={draft.shortcuts[key]}
                field={key}
                waitingFor={waitingFor}
                error={errors?.[key]}
                duplicate={duplicates[key]}
                onStart={startShortcutCapture}
                onStop={stopShortcutCapture}
              />
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

function ShortcutField({ label, value, field, waitingFor, disabled, error, duplicate, onStart, onStop }) {
  const active = waitingFor === field;
  return (
    <div
      className={field === 'reference-context' ? 'shortcut-field reference-shortcut-field' : 'shortcut-field'}
    >
      <label>{label}</label>
      <Input
        className={active ? 'shortcut-capturing' : ''}
        value={active ? '按下组合键…' : value}
        readOnly
        disabled={disabled}
        status={error ? 'error' : undefined}
        onClick={() => onStart(field)}
        onBlur={() => active && onStop()}
      />
      {error && <small className="shortcut-error">{error}</small>}
      {duplicate && <small className="shortcut-warning">与其他操作重复；运行时最后注册的操作生效。</small>}
    </div>
  );
}

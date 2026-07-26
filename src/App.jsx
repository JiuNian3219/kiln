import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button, Checkbox, Input, Modal, Radio, Select, Space, Spin } from 'antd';
import { CheckOutlined, CloseOutlined, ReloadOutlined, SettingOutlined } from '@ant-design/icons';
import { chooseDirectories, chooseMarkdownFile, invoke, listen } from './lib/tauri';
import { ActionBar, ControlPanel, PreviewBlock } from './components/PaletteParts';

const { TextArea } = Input;
const emptyContext = {
  agents: [],
  knowledgeBases: [],
  selectedAgentId: '',
  selectedKnowledgeBaseIds: [],
  combinations: [],
  selectedCombinationId: '',
  useAgent: false,
  useKnowledgeBase: false,
  useNetwork: false,
  networkAvailable: false,
  referenceText: null,
  referenceActive: false,
  referenceContextType: 'background',
  referenceContextNote: '',
};
const GENERAL_ENHANCEMENT_VALUE = '__general_enhancement__';
const referenceContextOptions = [
  { value: 'background', label: '背景资料' },
  { value: 'previous-ai-conversation', label: '先前的 AI 对话' },
  { value: 'external-material', label: '外部资料 / 文档' },
  { value: 'custom', label: '自定义说明' },
];
const referenceContextNotes = {
  'previous-ai-conversation': '这是先前的 AI 对话。当前选区是我对这段对话的最新回应。',
  'external-material': '这是外部资料或文档，只用于补充事实、术语和约束。',
};
const options = (items = []) => items.map((item) => ({ value: item.id, label: item.name }));
const asPanelPayload = (payload) => ({ ...payload, settings: { ...payload.settings, apiKey: '' } });
const preview = (text, limit = 240) => (text.length > limit ? `${text.slice(0, limit).trimEnd()}…` : text);
function App() {
  const [view, setView] = useState('palette');
  const [phase, setPhase] = useState('context');
  const [status, setStatus] = useState('在 Codex 中选中草稿后按 Ctrl+Alt+E。');
  const [original, setOriginal] = useState('');
  const [replacement, setReplacement] = useState('');
  const [context, setContext] = useState(emptyContext);
  const [questions, setQuestions] = useState([]);
  const [questionIndex, setQuestionIndex] = useState(0);
  const [answers, setAnswers] = useState({});
  const [busy, setBusy] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [candidate, setCandidate] = useState('');
  const [settings, setSettings] = useState(null);
  const [settingsNotice, setSettingsNotice] = useState('');
  const [settingsBusy, setSettingsBusy] = useState(false);
  const [canReturnToPalette, setCanReturnToPalette] = useState(false);
  const [diagnostics, setDiagnostics] = useState(null);
  const [featureErrors, setFeatureErrors] = useState({});

  const updateContext = (update) => setContext((current) => ({ ...current, ...update }));
  const currentQuestion = questions[questionIndex];
  const answerValue = (question) => {
    const answer = answers[question?.id] || {};
    const value = answer.custom?.trim() || answer.choice || '';
    return value ? `${question.prompt}：${value}` : '';
  };
  const answersComplete = useMemo(
    () =>
      questions.every((question) => {
        const answer = answers[question?.id] || {};
        return Boolean(answer.custom?.trim() || answer.choice);
      }),
    [questions, answers],
  );
  const sessionInput = (candidateMode = false) => ({
    useAgent: Boolean(context.selectedCombinationId),
    useKnowledgeBase: Boolean(context.selectedCombinationId),
    useNetwork: context.networkAvailable,
    agentId: context.selectedAgentId,
    knowledgeBaseIds: context.selectedKnowledgeBaseIds,
    answers: questions.map(answerValue).filter(Boolean),
    candidate: candidateMode,
    useReference: Boolean(context.referenceText && context.referenceActive),
    referenceContextType: context.referenceContextType,
    referenceContextNote: context.referenceContextNote.trim(),
  });
  const resetSession = (payload) => {
    setOriginal(payload.original || '');
    setReplacement('');
    setCandidate('');
    setContext({ ...emptyContext, ...payload });
    setQuestions([]);
    setAnswers({});
    setQuestionIndex(0);
    setPhase('context');
    setBusy(false);
  };
  const acceptReplacement = useCallback(async () => {
    if (!replacement.trim()) return;
    setBusy(true);
    setStatus('正在替换选区…');
    try {
      await invoke('accept_replacement', { replacement });
    } catch (error) {
      setStatus(`替换失败：${error}`);
      setBusy(false);
    }
  }, [replacement]);
  useEffect(() => {
    const listeners = [
      listen('selection-captured', ({ payload }) => {
        resetSession(payload);
        setView('palette');
        setStatus('选择本次需要的上下文。');
      }),
      listen('agent-status', ({ payload }) => setStatus(payload)),
      listen('generation-chunk', ({ payload }) => setReplacement((text) => text + payload)),
      listen('regeneration-chunk', ({ payload }) => setCandidate((text) => text + payload)),
      listen('capture-error', ({ payload }) => {
        setView('palette');
        setPhase('context');
        setStatus(`读取失败：${payload}`);
        setBusy(false);
      }),
      listen('settings-opened', ({ payload }) => {
        setSettings(asPanelPayload(payload));
        setSettingsNotice('');
        setFeatureErrors({});
        setCanReturnToPalette(false);
        setView('control');
        invoke('get_diagnostics')
          .then(setDiagnostics)
          .catch(() => setDiagnostics(null));
      }),
    ];
    return () => listeners.forEach((listener) => listener.then((unlisten) => unlisten()));
  }, []);
  useEffect(() => {
    const keydown = (event) => {
      if (view !== 'palette') return;
      if (event.key === 'Escape') {
        event.preventDefault();
        cancel();
      }
      if (
        event.key === 'Enter' &&
        !event.shiftKey &&
        !event.ctrlKey &&
        phase === 'preview' &&
        replacement.trim() &&
        !busy
      ) {
        event.preventDefault();
        acceptReplacement();
      }
      if (event.key === 'Tab' && phase === 'preview' && !detailsOpen) {
        event.preventDefault();
        setDetailsOpen((open) => !open);
      }
    };
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, [view, phase, replacement, busy, detailsOpen, acceptReplacement]);

  async function beginAnalysis() {
    setBusy(true);
    setStatus('正在分析草稿与上下文…');
    try {
      const result = await invoke('analyze_session', { input: sessionInput() });
      if (result.questions?.length) {
        setQuestions(result.questions);
        setAnswers({});
        setQuestionIndex(0);
        setPhase('questions');
        setStatus('补充一项必要信息。');
      } else await generateReplacement(false);
    } catch (error) {
      setStatus(`分析失败：${error}`);
    } finally {
      setBusy(false);
    }
  }
  async function generateReplacement(candidateMode) {
    if (!candidateMode && questions.length && !answersComplete) {
      setStatus('请先回答当前问题。');
      return;
    }
    setBusy(true);
    setStatus(candidateMode ? '正在重新生成…' : '正在生成替换文本…');
    if (candidateMode) setCandidate('');
    else setReplacement('');
    try {
      const result = await invoke('generate_replacement', { input: sessionInput(candidateMode) });
      if (candidateMode) {
        setCandidate(result);
        setDetailsOpen(true);
        setStatus('已生成候选文本。');
      } else {
        setReplacement(result);
        setPhase('preview');
        setStatus('检查结果后确认替换。');
      }
    } catch (error) {
      if (String(error).includes('textual tool syntax')) {
        if (candidateMode) setCandidate('');
        else setReplacement('');
      }
      setStatus(`生成失败：${error}`);
    } finally {
      if (!candidateMode) {
        updateContext({
          referenceText: null,
          referenceActive: false,
          referenceContextType: 'background',
          referenceContextNote: '',
        });
      }
      setBusy(false);
    }
  }
  function nextQuestion() {
    if (!answerValue(currentQuestion)) {
      setStatus('选择一个选项，或输入你的答案。');
      return;
    }
    if (questionIndex + 1 < questions.length) {
      setQuestionIndex((index) => index + 1);
      setStatus('补充下一项信息。');
    } else generateReplacement(false);
  }
  async function cancel() {
    await invoke('cancel_preview');
    await invoke('hide_main_window');
  }
  async function openControl() {
    try {
      const [panel, diagnosticPayload] = await Promise.all([
        invoke('get_settings'),
        invoke('get_diagnostics'),
      ]);
      setSettings(asPanelPayload(panel));
      setDiagnostics(diagnosticPayload);
      setSettingsNotice('');
      setFeatureErrors({});
      setCanReturnToPalette(true);
      setView('control');
    } catch (error) {
      setStatus(`无法打开控制面板：${error}`);
    }
  }
  async function refreshDiagnostics(notice = '') {
    setSettingsBusy(true);
    try {
      setDiagnostics(await invoke('get_diagnostics'));
      if (notice) setSettingsNotice(notice);
    } catch (error) {
      setSettingsNotice(`无法读取诊断日志：${error}`);
    } finally {
      setSettingsBusy(false);
    }
  }
  async function clearDiagnostics() {
    setSettingsBusy(true);
    try {
      setDiagnostics(await invoke('clear_diagnostics'));
      setSettingsNotice('本地诊断日志已清除。');
    } catch (error) {
      setSettingsNotice(`无法清除诊断日志：${error}`);
    } finally {
      setSettingsBusy(false);
    }
  }
  async function copyDiagnostics() {
    if (!diagnostics?.report) return;
    try {
      await navigator.clipboard.writeText(diagnostics.report);
      setSettingsNotice('诊断摘要已复制，可随 Bug 报告一并发送。');
    } catch (error) {
      setSettingsNotice(`无法复制诊断摘要：${error}`);
    }
  }
  async function saveFeatureAndShortcutSettings(input) {
    setSettingsBusy(true);
    setFeatureErrors({});
    try {
      const result = await invoke('save_feature_and_shortcut_settings', { input });
      if (!result.success) {
        setFeatureErrors(result.fieldErrors || {});
        setSettingsNotice('请修正标红的快捷键后重新保存。');
        return;
      }
      setSettings((current) => ({
        ...current,
        settings: {
          ...current.settings,
          ...result.settings,
          allowNetwork: Boolean(result.settings.featureToggles?.['network-search']),
        },
      }));
      setSettingsNotice('功能与快捷键已保存并立即生效。');
    } catch (error) {
      setSettingsNotice(`无法保存功能与快捷键：${error}`);
    } finally {
      setSettingsBusy(false);
    }
  }
  async function refreshSettings(action, notice) {
    setSettingsBusy(true);
    try {
      setSettings(asPanelPayload(await action()));
      setSettingsNotice(notice);
      return true;
    } catch (error) {
      setSettingsNotice(`操作失败：${error}`);
      return false;
    } finally {
      setSettingsBusy(false);
    }
  }
  async function importAgent() {
    const sourcePath = await chooseMarkdownFile('选择 Agent Markdown 文件');
    if (typeof sourcePath === 'string')
      await refreshSettings(() => invoke('import_agent', { sourcePath }), 'Agent 已导入。');
  }
  async function importKnowledgeBases() {
    const selected = await chooseDirectories('选择一个或多个知识库文件夹');
    const sourcePaths = Array.isArray(selected) ? selected : selected ? [selected] : [];
    if (sourcePaths.length)
      await refreshSettings(
        () => invoke('import_knowledge_bases', { sourcePaths }),
        `已导入 ${sourcePaths.length} 个知识库。`,
      );
  }
  if (view === 'control' && settings)
    return (
      <ControlPanel
        payload={settings}
        busy={settingsBusy}
        notice={settingsNotice}
        canReturn={canReturnToPalette}
        onBack={() => {
          setSettingsNotice('');
          setCanReturnToPalette(false);
          setView('palette');
        }}
        onClose={() => invoke('hide_main_window')}
        onChange={(field, value) =>
          setSettings((current) => ({ ...current, settings: { ...current.settings, [field]: value } }))
        }
        onImportAgent={importAgent}
        onImportKnowledgeBases={importKnowledgeBases}
        onDelete={(command, id, label) => refreshSettings(() => invoke(command, { id }), `${label} 已删除。`)}
        onSaveCombination={(input) =>
          refreshSettings(() => invoke('save_combination', { input }), '组合已保存。')
        }
        onDeleteCombination={(id) =>
          refreshSettings(() => invoke('delete_combination', { id }), '组合已删除。')
        }
        onSetDefaultCombination={(id) =>
          refreshSettings(() => invoke('set_default_combination', { id }), '默认组合已更新。')
        }
        onGenerateKnowledgeBaseIndex={(id) =>
          refreshSettings(
            () => invoke('generate_knowledge_base_index', { id }),
            'AI 索引已生成并保存到应用私有目录。',
          )
        }
        onSpecifyKnowledgeBaseIndex={async (id) => {
          try {
            const candidates = await invoke('get_knowledge_base_index_candidates', { id });
            const selected = window.prompt(
              `请输入索引文件的相对路径：\n${candidates.join('\n')}`,
              candidates.find((item) => item.toLowerCase() === 'index.md') || candidates[0] || '',
            );
            if (selected)
              await refreshSettings(
                () => invoke('set_knowledge_base_index', { id, mode: 'manual', manualPath: selected }),
                '已手动指定知识库索引。',
              );
          } catch (error) {
            setSettingsNotice(`无法读取知识库文件：${error}`);
          }
        }}
        onSave={() =>
          refreshSettings(
            () =>
              invoke('save_settings', {
                input: { ...settings.settings, apiKey: settings.settings.apiKey || '' },
              }),
            '配置已保存。',
          )
        }
        onTest={async () => {
          setSettingsBusy(true);
          try {
            setSettingsNotice(`API 测试成功：${await invoke('test_deepseek_connection')}`);
          } catch (error) {
            setSettingsNotice(`API 测试失败：${error}`);
          } finally {
            setSettingsBusy(false);
          }
        }}
        diagnostics={diagnostics}
        onRefreshDiagnostics={() => refreshDiagnostics('诊断日志已刷新。')}
        onCopyDiagnostics={copyDiagnostics}
        onClearDiagnostics={clearDiagnostics}
        featureErrors={featureErrors}
        onFeatureShortcutSave={saveFeatureAndShortcutSettings}
      />
    );

  return (
    <main className="palette" aria-live="polite">
      <header className="palette-titlebar">
        <div className="palette-drag-region" data-tauri-drag-region>
          <span className="app-mark">CODEX INPUT ENHANCER</span>
        </div>
        <div data-no-window-drag>
          <Button
            type="text"
            size="small"
            icon={<SettingOutlined />}
            onClick={openControl}
            aria-label="控制面板"
          />
          <Button type="text" size="small" icon={<CloseOutlined />} onClick={cancel} aria-label="取消" />
        </div>
      </header>
      <div className="palette-status">
        {busy && <Spin size="small" />}
        {status}
      </div>
      {phase === 'context' && (
        <section className="palette-body context-body">
          <div className="context-scroll">
            {context.referenceText && (
              <div className="reference-context">
                <div>
                  <label>参考上下文</label>
                  <span>{preview(context.referenceText, 80)}</span>
                </div>
                <Checkbox
                  checked={context.referenceActive}
                  disabled={busy}
                  onChange={(event) => updateContext({ referenceActive: event.target.checked })}
                >
                  作为参考附带
                </Checkbox>
                <Button
                  type="text"
                  size="small"
                  aria-label="清除参考上下文"
                  onClick={async () => {
                    await invoke('clear_reference');
                    updateContext({
                      referenceText: null,
                      referenceActive: false,
                      referenceContextType: 'background',
                      referenceContextNote: '',
                    });
                  }}
                >
                  ×
                </Button>
                {context.referenceActive && (
                  <div className="reference-context-guidance">
                    <Select
                      size="small"
                      value={context.referenceContextType}
                      options={referenceContextOptions}
                      disabled={busy}
                      aria-label="参考上下文用途"
                      onChange={(value) =>
                        updateContext({
                          referenceContextType: value,
                          referenceContextNote:
                            referenceContextNotes[value] ||
                            (value === 'background' ? '' : context.referenceContextNote),
                        })
                      }
                    />
                    <Input
                      size="small"
                      value={context.referenceContextNote}
                      disabled={busy}
                      maxLength={500}
                      aria-label="参考上下文说明"
                      placeholder="可补充说明它与当前草稿的关系"
                      onChange={(event) => updateContext({ referenceContextNote: event.target.value })}
                    />
                  </div>
                )}
              </div>
            )}
            <label className="field-label">当前草稿</label>
            <div className="draft-line">{preview(original, 120) || '等待读取选区…'}</div>
            <div className="field-row">
              <label>工作组合</label>
              <Select
                size="small"
                value={context.selectedCombinationId || GENERAL_ENHANCEMENT_VALUE}
                options={[
                  { value: GENERAL_ENHANCEMENT_VALUE, label: '通用增强' },
                  ...options(context.combinations),
                ]}
                disabled={busy}
                onChange={(value) => {
                  if (value === GENERAL_ENHANCEMENT_VALUE) {
                    updateContext({
                      selectedCombinationId: '',
                      selectedAgentId: '',
                      selectedKnowledgeBaseIds: [],
                    });
                    return;
                  }
                  const combination = context.combinations.find((item) => item.id === value);
                  updateContext({
                    selectedCombinationId: value,
                    selectedAgentId: combination?.agentId || '',
                    selectedKnowledgeBaseIds: combination?.knowledgeBaseIds || [],
                  });
                }}
              />
            </div>
          </div>
          <ActionBar>
            <Button onClick={cancel}>取消</Button>
            <Button type="primary" loading={busy} onClick={beginAnalysis}>
              继续
            </Button>
          </ActionBar>
        </section>
      )}
      {phase === 'questions' && currentQuestion && (
        <section className="palette-body question-body">
          <div className="question-scroll">
            <div className="step-hint">
              需要补充信息 · {questionIndex + 1}/{questions.length}
            </div>
            <h2>{currentQuestion.prompt}</h2>
            {currentQuestion.options?.length ? (
              <Radio.Group
                value={answers[currentQuestion.id]?.choice}
                onChange={(event) =>
                  setAnswers((current) => ({
                    ...current,
                    [currentQuestion.id]: { ...current[currentQuestion.id], choice: event.target.value },
                  }))
                }
              >
                <Space direction="vertical">
                  {currentQuestion.options.map((option) => (
                    <Radio key={option} value={option}>
                      {option}
                    </Radio>
                  ))}
                </Space>
              </Radio.Group>
            ) : null}
            <Input
              size="small"
              value={answers[currentQuestion.id]?.custom || ''}
              onChange={(event) =>
                setAnswers((current) => ({
                  ...current,
                  [currentQuestion.id]: { ...current[currentQuestion.id], custom: event.target.value },
                }))
              }
              placeholder="或直接输入你的答案（优先采用）"
            />
          </div>
          <ActionBar>
            <Button
              onClick={() => (questionIndex ? setQuestionIndex((index) => index - 1) : setPhase('context'))}
              disabled={busy}
            >
              返回
            </Button>
            <Button type="primary" loading={busy} onClick={nextQuestion}>
              {questionIndex + 1 === questions.length ? '生成' : '下一项'}
            </Button>
          </ActionBar>
        </section>
      )}
      {phase === 'preview' && (
        <section className="palette-body preview-body">
          <div className="compare-grid">
            <PreviewBlock label="原文" text={original} />
            <PreviewBlock label="建议" text={replacement} accent />
          </div>
          <div className="key-hint">Enter 确认替换 · Tab 查看完整对比 · Esc 取消</div>
          <ActionBar>
            <Button onClick={cancel}>取消</Button>
            <Button onClick={() => setDetailsOpen(true)}>查看完整对比</Button>
            <Button icon={<ReloadOutlined />} onClick={() => generateReplacement(true)} loading={busy}>
              重新生成
            </Button>
            <Button type="primary" icon={<CheckOutlined />} onClick={acceptReplacement} loading={busy}>
              替换
            </Button>
          </ActionBar>
        </section>
      )}
      <Modal
        open={detailsOpen}
        title="完整对比"
        footer={
          <Space>
            <Button onClick={() => setDetailsOpen(false)}>关闭</Button>
            <Button
              type="primary"
              onClick={() => {
                if (candidate) setReplacement(candidate);
                setDetailsOpen(false);
              }}
            >
              采用候选文本
            </Button>
          </Space>
        }
        onCancel={() => setDetailsOpen(false)}
        centered
        className="diff-modal"
      >
        <div className="full-diff">
          <div>
            <label>原文</label>
            <pre>{original}</pre>
          </div>
          <div>
            <label>建议</label>
            <TextArea
              value={candidate || replacement}
              onChange={(event) =>
                candidate ? setCandidate(event.target.value) : setReplacement(event.target.value)
              }
              autoSize={{ minRows: 8, maxRows: 12 }}
            />
          </div>
        </div>
      </Modal>
    </main>
  );
}

export default App;

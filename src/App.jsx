import { useEffect, useMemo, useState } from 'react';
import {
  Button, Checkbox, Input, Modal, Radio, Select, Space, Spin,
} from 'antd';
import {
  CheckOutlined, CloseOutlined, ReloadOutlined, SettingOutlined,
} from '@ant-design/icons';
import { chooseDirectory, invoke, listen } from './lib/tauri';
import { ActionBar, ControlPanel, PreviewBlock } from './components/PaletteParts';

const { TextArea } = Input;
const emptyContext = { agents: [], knowledgeBases: [], selectedAgentId: '', selectedKnowledgeBaseIds: [], useAgent: false, useKnowledgeBase: false, useNetwork: false, networkAvailable: false, referenceText: null, referenceActive: false };
const options = (items = []) => items.map((item) => ({ value: item.id, label: item.name }));
const asPanelPayload = (payload) => ({ ...payload, settings: { ...payload.settings, apiKey: '' } });
const preview = (text, limit = 240) => text.length > limit ? `${text.slice(0, limit).trimEnd()}…` : text;
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

  const updateContext = (update) => setContext((current) => ({ ...current, ...update }));
  const currentQuestion = questions[questionIndex];
  const answerValue = (question) => {
    const answer = answers[question?.id] || {};
    const value = answer.custom?.trim() || answer.choice || '';
    return value ? `${question.prompt}：${value}` : '';
  };
  const answersComplete = useMemo(() => questions.every((question) => answerValue(question)), [questions, answers]);
  const sessionInput = (candidateMode = false) => ({
    useAgent: context.useAgent,
    useKnowledgeBase: context.useKnowledgeBase,
    useNetwork: context.networkAvailable && context.useNetwork,
    agentId: context.selectedAgentId,
    knowledgeBaseIds: context.selectedKnowledgeBaseIds,
    answers: questions.map(answerValue).filter(Boolean),
    candidate: candidateMode,
    useReference: Boolean(context.referenceText && context.referenceActive),
  });
  const updateAnswer = (update) => setAnswers((current) => ({ ...current, [currentQuestion.id]: { ...current[currentQuestion.id], ...update } }));

  const resetSession = (payload) => {
    setOriginal(payload.original || ''); setReplacement(''); setCandidate('');
    setContext({ ...emptyContext, ...payload }); setQuestions([]); setAnswers({}); setQuestionIndex(0);
    setPhase('context'); setBusy(false);
  };
  useEffect(() => {
    const listeners = [
      listen('selection-captured', ({ payload }) => { resetSession(payload); setView('palette'); setStatus('选择本次需要的上下文。'); }),
      listen('agent-status', ({ payload }) => setStatus(payload)),
      listen('generation-chunk', ({ payload }) => setReplacement((text) => text + payload)),
      listen('regeneration-chunk', ({ payload }) => setCandidate((text) => text + payload)),
      listen('capture-error', ({ payload }) => { setView('palette'); setPhase('context'); setStatus(`读取失败：${payload}`); setBusy(false); }),
      listen('settings-opened', ({ payload }) => { setSettings(asPanelPayload(payload)); setSettingsNotice(''); setView('control'); }),
    ];
    return () => listeners.forEach((listener) => listener.then((unlisten) => unlisten()));
  }, []);
  useEffect(() => {
    const keydown = (event) => {
      if (view !== 'palette') return;
      if (event.key === 'Escape') { event.preventDefault(); cancel(); }
      if (event.key === 'Enter' && !event.shiftKey && !event.ctrlKey && phase === 'preview' && replacement.trim() && !busy) {
        event.preventDefault(); acceptReplacement();
      }
      if (event.key === 'Tab' && phase === 'preview' && !detailsOpen) {
        event.preventDefault(); setDetailsOpen((open) => !open);
      }
    };
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, [view, phase, replacement, busy, detailsOpen]);

  async function beginAnalysis() {
    setBusy(true); setStatus('正在分析草稿与上下文…');
    try {
      const result = await invoke('analyze_session', { input: sessionInput() });
      if (result.questions?.length) {
        setQuestions(result.questions); setAnswers({}); setQuestionIndex(0); setPhase('questions');
        setStatus('补充一项必要信息。');
      } else await generateReplacement(false);
    } catch (error) { setStatus(`分析失败：${error}`); }
    finally { setBusy(false); }
  }
  async function generateReplacement(candidateMode) {
    if (!candidateMode && questions.length && !answersComplete) { setStatus('请先回答当前问题。'); return; }
    setBusy(true); setStatus(candidateMode ? '正在重新生成…' : '正在生成替换文本…');
    if (candidateMode) setCandidate(''); else setReplacement('');
    try {
      const result = await invoke('generate_replacement', { input: sessionInput(candidateMode) });
      if (candidateMode) { setCandidate(result); setDetailsOpen(true); setStatus('已生成候选文本。'); }
      else { setReplacement(result); setPhase('preview'); setStatus('检查结果后确认替换。'); }
    } catch (error) {
      if (String(error).includes('textual tool syntax')) {
        if (candidateMode) setCandidate(''); else setReplacement('');
      }
      setStatus(`生成失败：${error}`);
    }
    finally {
      if (!candidateMode) updateContext({ referenceText: null, referenceActive: false });
      setBusy(false);
    }
  }
  function nextQuestion() {
    if (!answerValue(currentQuestion)) { setStatus('选择一个选项，或输入你的答案。'); return; }
    if (questionIndex + 1 < questions.length) { setQuestionIndex((index) => index + 1); setStatus('补充下一项信息。'); }
    else generateReplacement(false);
  }
  async function acceptReplacement() {
    if (!replacement.trim()) return;
    setBusy(true); setStatus('正在替换选区…');
    try { await invoke('accept_replacement', { replacement }); }
    catch (error) { setStatus(`替换失败：${error}`); setBusy(false); }
  }
  async function cancel() { await invoke('cancel_preview'); await invoke('hide_main_window'); }
  async function openControl() {
    try { setSettings(asPanelPayload(await invoke('get_settings'))); setSettingsNotice(''); setView('control'); }
    catch (error) { setStatus(`无法打开控制面板：${error}`); }
  }
  async function chooseDirectory(field, title) {
    const value = await chooseDirectory(title);
    if (typeof value === 'string') setSettings((current) => ({ ...current, settings: { ...current.settings, [field]: value } }));
  }
  async function refreshSettings(action, notice) {
    setSettingsBusy(true);
    try { setSettings(asPanelPayload(await action())); setSettingsNotice(notice); }
    catch (error) { setSettingsNotice(`操作失败：${error}`); }
    finally { setSettingsBusy(false); }
  }
  async function importCatalog(command, title, label) {
    const sourcePath = await chooseDirectory(title);
    if (typeof sourcePath === 'string') await refreshSettings(() => invoke(command, { sourcePath }), `${label} 已导入。`);
  }

  if (view === 'control' && settings) return <ControlPanel
    payload={settings} busy={settingsBusy} notice={settingsNotice} onClose={() => invoke('hide_main_window')}
    onChange={(field, value) => setSettings((current) => ({ ...current, settings: { ...current.settings, [field]: value } }))}
    onChooseDirectory={chooseDirectory} onImport={importCatalog}
    onDelete={(command, id, label) => refreshSettings(() => invoke(command, { id }), `${label} 已删除。`)}
    onSave={() => refreshSettings(() => invoke('save_settings', { input: { ...settings.settings, apiKey: settings.settings.apiKey || '' } }), '配置已保存。')}
    onTest={async () => { setSettingsBusy(true); try { setSettingsNotice(`API 测试成功：${await invoke('test_deepseek_connection')}`); } catch (error) { setSettingsNotice(`API 测试失败：${error}`); } finally { setSettingsBusy(false); } }}
  />;

  return <main className="palette" aria-live="polite">
    <header className="palette-titlebar"><div className="palette-drag-region" data-tauri-drag-region><span className="app-mark">CODEX INPUT ENHANCER</span></div><div data-no-window-drag><Button type="text" size="small" icon={<SettingOutlined />} onClick={openControl} aria-label="控制面板" /><Button type="text" size="small" icon={<CloseOutlined />} onClick={cancel} aria-label="取消" /></div></header>
    <div className="palette-status">{busy && <Spin size="small" />}{status}</div>
    {phase === 'context' && <section className="palette-body">
      {context.referenceText && <div className="reference-context"><div><label>Reference context</label><span>{preview(context.referenceText, 80)}</span></div><Checkbox checked={context.referenceActive} disabled={busy} onChange={(event) => updateContext({ referenceActive: event.target.checked })}>Attach as reference</Checkbox><Button type="text" size="small" aria-label="Clear reference" onClick={async () => { await invoke('clear_reference'); updateContext({ referenceText: null, referenceActive: false }); }}>×</Button></div>}
      <label className="field-label">当前草稿</label><div className="draft-line">{preview(original, 120) || '等待读取选区…'}</div>
      <div className="field-row"><Checkbox checked={context.useAgent} disabled={busy || !context.agents.length} onChange={(event) => updateContext({ useAgent: event.target.checked })}>Agent</Checkbox><Select size="small" value={context.selectedAgentId || undefined} placeholder="不使用" options={options(context.agents)} disabled={busy || !context.useAgent} onChange={(value) => updateContext({ selectedAgentId: value })} /></div>
      <div className="field-row kb-row"><Checkbox checked={context.useKnowledgeBase} disabled={busy || !context.knowledgeBases.length} onChange={(event) => updateContext({ useKnowledgeBase: event.target.checked })}>知识库</Checkbox><Select size="small" mode="multiple" value={context.selectedKnowledgeBaseIds} placeholder="不使用" options={options(context.knowledgeBases)} disabled={busy || !context.useKnowledgeBase} onChange={(value) => updateContext({ selectedKnowledgeBaseIds: value })} maxTagCount="responsive" /></div>
      {context.networkAvailable && <Checkbox className="network-check" checked={context.useNetwork} disabled={busy} onChange={(event) => updateContext({ useNetwork: event.target.checked })}>本次允许联网</Checkbox>}
      <ActionBar><Button onClick={cancel}>取消</Button><Button type="primary" loading={busy} onClick={beginAnalysis}>继续</Button></ActionBar>
    </section>}
    {phase === 'questions' && currentQuestion && <section className="palette-body question-body">
      <div className="step-hint">需要补充信息 · {questionIndex + 1}/{questions.length}</div><h2>{currentQuestion.prompt}</h2>
      {currentQuestion.options?.length ? <Radio.Group value={answers[currentQuestion.id]?.choice} onChange={(event) => setAnswers((current) => ({ ...current, [currentQuestion.id]: { ...current[currentQuestion.id], choice: event.target.value } }))}><Space direction="vertical">{currentQuestion.options.map((option) => <Radio key={option} value={option}>{option}</Radio>)}</Space></Radio.Group> : null}
      <Input size="small" value={answers[currentQuestion.id]?.custom || ''} onChange={(event) => setAnswers((current) => ({ ...current, [currentQuestion.id]: { ...current[currentQuestion.id], custom: event.target.value } }))} placeholder="或直接输入你的答案（优先采用）" />
      <ActionBar><Button onClick={() => questionIndex ? setQuestionIndex((index) => index - 1) : setPhase('context')} disabled={busy}>返回</Button><Button type="primary" loading={busy} onClick={nextQuestion}>{questionIndex + 1 === questions.length ? '生成' : '下一项'}</Button></ActionBar>
    </section>}
    {phase === 'preview' && <section className="palette-body preview-body">
      <div className="compare-grid"><PreviewBlock label="原文" text={original} /><PreviewBlock label="建议" text={replacement} accent /></div>
      <div className="key-hint">Enter 确认替换　·　Tab 查看完整对比　·　Esc 取消</div>
      <ActionBar><Button onClick={cancel}>取消</Button><Button onClick={() => setDetailsOpen(true)}>查看完整对比</Button><Button icon={<ReloadOutlined />} onClick={() => generateReplacement(true)} loading={busy}>重新生成</Button><Button type="primary" icon={<CheckOutlined />} onClick={acceptReplacement} loading={busy}>替换</Button></ActionBar>
    </section>}
    <Modal open={detailsOpen} title="完整对比" footer={<Space><Button onClick={() => setDetailsOpen(false)}>关闭</Button><Button type="primary" onClick={() => { if (candidate) setReplacement(candidate); setDetailsOpen(false); }}>采用候选文本</Button></Space>} onCancel={() => setDetailsOpen(false)} centered className="diff-modal">
      <div className="full-diff"><div><label>原文</label><pre>{original}</pre></div><div><label>建议</label><TextArea value={candidate || replacement} onChange={(event) => candidate ? setCandidate(event.target.value) : setReplacement(event.target.value)} autoSize={{ minRows: 8, maxRows: 12 }} /></div></div>
    </Modal>
  </main>;
}

export default App;

import { useCallback, useEffect, useState } from 'react';
import { invoke, listen, reportClientDiagnostic } from '../../lib/tauri';

const GENERAL_ENHANCEMENT_AGENT_ID = '__general_enhancement__';
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

export function useRewriteSession(view, setView) {
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
  const updateContext = (update) => setContext((current) => ({ ...current, ...update }));
  const currentQuestion = questions[questionIndex];
  const answerValue = (question) => {
    const answer = answers[question?.id] || {};
    const value = answer.custom?.trim() || answer.choice || '';
    return value ? `${question.prompt}：${value}` : '';
  };
  const answersComplete = questions.every((question) => Boolean(answerValue(question)));
  const sessionInput = (candidateMode = false) => ({
    useAgent:
      Boolean(context.selectedCombinationId) && context.selectedAgentId !== GENERAL_ENHANCEMENT_AGENT_ID,
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
  const cancel = useCallback(async () => {
    await reportClientDiagnostic('preview_cancel_clicked').catch(() => {});
    try {
      await invoke('cancel_preview');
    } catch (error) {
      await reportClientDiagnostic('preview_cancel_failed', error?.name).catch(() => {});
      setStatus('取消失败，请重试。');
    }
  }, []);
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
        setStatus('选择本次增强的工作组合。');
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
    ];
    return () => listeners.forEach((listener) => listener.then((unlisten) => unlisten()));
  }, [setView]);
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
      const focused =
        event.target instanceof Element &&
        event.target.closest('button, input, textarea, select, [role="button"]');
      if (event.key === 'Tab' && phase === 'preview' && !detailsOpen && !focused) {
        event.preventDefault();
        setDetailsOpen((open) => !open);
      }
    };
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, [view, phase, replacement, busy, detailsOpen, acceptReplacement, cancel]);

  async function beginAnalysis() {
    await reportClientDiagnostic('preview_continue_clicked').catch(() => {});
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
      } else if (result.replacement?.trim()) {
        setReplacement(result.replacement);
        setPhase('preview');
        setStatus('检查结果后确认替换。');
        clearReference();
      } else setStatus('分析结果缺少替换文本，请重试。');
    } catch (error) {
      setStatus(`分析失败：${error}`);
    } finally {
      setBusy(false);
    }
  }
  function clearReference() {
    updateContext({
      referenceText: null,
      referenceActive: false,
      referenceContextType: 'background',
      referenceContextNote: '',
    });
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
      if (!candidateMode) clearReference();
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
  return {
    status,
    busy,
    phase,
    original,
    replacement,
    context,
    currentQuestion,
    questionIndex,
    questions,
    answers,
    detailsOpen,
    candidate,
    setCandidate,
    setReplacement,
    acceptReplacement,
    beginAnalysis,
    cancel,
    onContextChange: updateContext,
    onClearReference: async () => {
      await invoke('clear_reference');
      clearReference();
    },
    onAnswer: (id, answer) => setAnswers((current) => ({ ...current, [id]: { ...current[id], ...answer } })),
    onQuestionBack: () => (questionIndex ? setQuestionIndex((index) => index - 1) : setPhase('context')),
    onQuestionNext: nextQuestion,
    onDetailsOpen: () => setDetailsOpen(true),
    onDetailsClose: () => setDetailsOpen(false),
    onCandidateChange: setCandidate,
    onReplacementChange: setReplacement,
    onAdoptCandidate: () => {
      if (candidate) setReplacement(candidate);
      setDetailsOpen(false);
    },
    onRegenerate: () => generateReplacement(true),
    setStatus,
  };
}

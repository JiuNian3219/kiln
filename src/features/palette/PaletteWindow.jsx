import { Button, Input, Modal, Space, Spin } from 'antd';
import { SettingOutlined } from '@ant-design/icons';
import { WindowTitlebar } from '../../components/WindowTitlebar';
import { ClarificationStep } from './ClarificationStep';
import { ContextStep } from './ContextStep';
import { PreviewStep } from './PreviewStep';
import './PaletteWindow.css';

const { TextArea } = Input;

export function PaletteWindow({
  phase,
  status,
  busy,
  original,
  replacement,
  context,
  currentQuestion,
  questionIndex,
  questions,
  answers,
  detailsOpen,
  candidate,
  onOpenControl,
  onCancel,
  onContextChange,
  onClearReference,
  onBeginAnalysis,
  onAnswer,
  onQuestionBack,
  onQuestionNext,
  onDetailsOpen,
  onDetailsClose,
  onCandidateChange,
  onReplacementChange,
  onAdoptCandidate,
  onRegenerate,
  onAccept,
}) {
  return (
    <main className="palette" aria-live="polite">
      <WindowTitlebar label="CODEX INPUT ENHANCER" onClose={onCancel} closeLabel="取消">
        <Button
          type="text"
          size="small"
          icon={<SettingOutlined />}
          onClick={onOpenControl}
          aria-label="控制面板"
        />
      </WindowTitlebar>
      <div className="palette-status">
        {busy && <Spin size="small" />}
        {status}
      </div>
      {phase === 'context' && (
        <ContextStep
          context={context}
          original={original}
          busy={busy}
          onChange={onContextChange}
          onClearReference={onClearReference}
          onCancel={onCancel}
          onContinue={onBeginAnalysis}
        />
      )}
      {phase === 'questions' && currentQuestion && (
        <ClarificationStep
          question={currentQuestion}
          questionIndex={questionIndex}
          questionCount={questions.length}
          answers={answers}
          busy={busy}
          onAnswer={onAnswer}
          onBack={onQuestionBack}
          onNext={onQuestionNext}
        />
      )}
      {phase === 'preview' && (
        <PreviewStep
          original={original}
          replacement={replacement}
          busy={busy}
          onCancel={onCancel}
          onViewDetails={onDetailsOpen}
          onRegenerate={onRegenerate}
          onAccept={onAccept}
        />
      )}
      <Modal
        open={detailsOpen}
        title="完整对比"
        footer={
          <Space>
            <Button onClick={onDetailsClose}>关闭</Button>
            <Button type="primary" onClick={onAdoptCandidate}>
              采用候选文本
            </Button>
          </Space>
        }
        onCancel={onDetailsClose}
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
                candidate ? onCandidateChange(event.target.value) : onReplacementChange(event.target.value)
              }
              autoSize={{ minRows: 8, maxRows: 12 }}
            />
          </div>
        </div>
      </Modal>
    </main>
  );
}

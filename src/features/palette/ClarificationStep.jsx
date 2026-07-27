import { Button, Input, Radio, Space } from 'antd';
import { ActionBar } from '../../components/ActionBar';
import './ClarificationStep.css';

export function ClarificationStep({
  question,
  questionIndex,
  questionCount,
  answers,
  busy,
  onAnswer,
  onBack,
  onNext,
}) {
  return (
    <section className="palette-body question-body">
      <div className="question-scroll">
        <div className="step-hint">
          需要补充信息 · {questionIndex + 1}/{questionCount}
        </div>
        <h2>{question.prompt}</h2>
        {question.options?.length ? (
          <Radio.Group
            value={answers[question.id]?.choice}
            onChange={(event) => onAnswer(question.id, { choice: event.target.value })}
          >
            <Space direction="vertical">
              {question.options.map((option) => (
                <Radio key={option} value={option}>
                  {option}
                </Radio>
              ))}
            </Space>
          </Radio.Group>
        ) : null}
        <Input
          size="small"
          value={answers[question.id]?.custom || ''}
          onChange={(event) => onAnswer(question.id, { custom: event.target.value })}
          placeholder="或直接输入你的答案（优先采用）"
        />
      </div>
      <ActionBar>
        <Button onClick={onBack} disabled={busy}>
          返回
        </Button>
        <Button type="primary" loading={busy} onClick={onNext}>
          {questionIndex + 1 === questionCount ? '生成' : '下一项'}
        </Button>
      </ActionBar>
    </section>
  );
}

import { Button, Checkbox, Input, Select } from 'antd';
import { ActionBar } from '../../components/ActionBar';
import './ContextStep.css';

const GENERAL_ENHANCEMENT_VALUE = '__general_enhancement__';
const referenceContextOptions = [
  { value: 'background', label: '背景资料' },
  { value: 'previous-ai-conversation', label: '先前的 AI 对话' },
  { value: 'external-material', label: '外部文档' },
  { value: 'custom', label: '自定义说明' },
];
const referenceContextNotes = {
  'previous-ai-conversation': '这是先前的 AI 对话。当前选区是我对这段对话的最新回应。',
  'external-material': '这是外部资料或文档，只用于补充事实、术语和约束。',
};
const preview = (text, limit) => (text.length > limit ? `${text.slice(0, limit).trimEnd()}…` : text);
const options = (items = []) => items.map((item) => ({ value: item.id, label: item.name }));

export function ContextStep({ context, original, busy, onChange, onClearReference, onCancel, onContinue }) {
  return (
    <section className="palette-body context-body">
      <div className="context-scroll">
        {context.referenceText && (
          <div className="reference-context">
            <label className="field-label">参考上下文</label>
            <div className="reference-context-content">
              <div className="reference-context-summary">
                <span>{preview(context.referenceText, 80)}</span>
                <div className="reference-context-actions">
                  <Checkbox
                    checked={context.referenceActive}
                    disabled={busy}
                    onChange={(event) => onChange({ referenceActive: event.target.checked })}
                  >
                    作为参考附带
                  </Checkbox>
                  <Button type="text" size="small" aria-label="清除参考上下文" onClick={onClearReference}>
                    ×
                  </Button>
                </div>
              </div>
              {context.referenceActive && (
                <div className="reference-context-guidance">
                  <Select
                    size="small"
                    value={context.referenceContextType}
                    options={referenceContextOptions}
                    disabled={busy}
                    aria-label="参考上下文用途"
                    onChange={(value) =>
                      onChange({
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
                    onChange={(event) => onChange({ referenceContextNote: event.target.value })}
                  />
                </div>
              )}
            </div>
          </div>
        )}
        <div className="context-field">
          <label className="field-label">当前草稿</label>
          <div className="draft-line">{preview(original, 120) || '等待读取选区…'}</div>
        </div>
        <div className="field-row">
          <label className="field-label">工作组合</label>
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
                onChange({ selectedCombinationId: '', selectedAgentId: '', selectedKnowledgeBaseIds: [] });
                return;
              }
              const combination = context.combinations.find((item) => item.id === value);
              onChange({
                selectedCombinationId: value,
                selectedAgentId: combination?.agentId || '',
                selectedKnowledgeBaseIds: combination?.knowledgeBaseIds || [],
              });
            }}
          />
        </div>
      </div>
      <ActionBar>
        <Button onClick={onCancel}>取消</Button>
        <Button type="primary" loading={busy} onClick={onContinue}>
          继续
        </Button>
      </ActionBar>
    </section>
  );
}

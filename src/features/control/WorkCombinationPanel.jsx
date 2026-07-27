import { useState } from 'react';
import { Button, Empty, Form, Input, List, Popconfirm, Select, Typography } from 'antd';
import { ArrowLeftOutlined, DeleteOutlined, EditOutlined, PlusOutlined } from '@ant-design/icons';
import { ActionBar } from '../../components/ActionBar';
import './WorkCombinationPanel.css';

const { Text } = Typography;

export function CombinationPanel({ payload, busy, onOpenEditor, onEdit, onDelete, onSetDefault }) {
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

export function CombinationEditorPage({ payload, combination, busy, onBack, onSave }) {
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

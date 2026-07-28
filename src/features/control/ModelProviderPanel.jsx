import { useState } from 'react';
import { Button, Empty, Form, Input, List, Popconfirm, Select, Typography } from 'antd';
import { ArrowLeftOutlined, DeleteOutlined, EditOutlined, PlusOutlined } from '@ant-design/icons';
import { ActionBar } from '../../components/ActionBar';
import './ModelProviderPanel.css';

const { Text } = Typography;
const protocols = [
  { value: 'openai-chat-completions', label: 'OpenAI Chat Completions' },
  { value: 'openai-responses', label: 'OpenAI Responses' },
  { value: 'anthropic-messages', label: 'Anthropic Messages' },
  { value: 'gemini-generate-content', label: 'Gemini GenerateContent' },
];
const protocolName = (value) => protocols.find((protocol) => protocol.value === value)?.label || value;
const newProvider = () => ({
  id: '',
  name: '',
  protocol: 'openai-chat-completions',
  baseUrl: '',
  model: '',
  apiKey: '',
});

export function ModelProviderPanel({ payload, busy, onSave, onDelete, onSetDefault, onTest }) {
  const [editing, setEditing] = useState(null);
  const providers = payload.settings.modelProviders || [];
  if (editing) {
    return (
      <ProviderEditor
        provider={editing}
        busy={busy}
        onBack={() => setEditing(null)}
        onSave={async (input) => {
          if (await onSave(input)) setEditing(null);
        }}
      />
    );
  }
  return (
    <section className="model-provider-panel">
      <div className="catalog-heading">
        <div>
          <Text>AI 服务</Text>
          <span>仅支持 API Key；服务协议与厂商预设相互独立。</span>
        </div>
        <Button
          type="primary"
          size="small"
          icon={<PlusOutlined />}
          disabled={busy}
          onClick={() => setEditing(newProvider())}
        >
          添加服务
        </Button>
      </div>
      <List
        className="provider-list"
        size="small"
        dataSource={providers}
        locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无 AI 服务" /> }}
        renderItem={(provider) => (
          <List.Item
            actions={[
              <Button key="test" type="text" size="small" disabled={busy} onClick={() => onTest(provider.id)}>
                测试
              </Button>,
              <Button
                key="edit"
                type="text"
                size="small"
                icon={<EditOutlined />}
                aria-label="编辑 AI 服务"
                disabled={busy}
                onClick={() => setEditing({ ...provider, apiKey: '' })}
              />,
              <Popconfirm
                key="delete"
                title={`删除「${provider.name}」？`}
                description="同时删除其 API Key。"
                okText="删除"
                cancelText="取消"
                onConfirm={() => onDelete(provider.id)}
              >
                <Button danger type="text" size="small" icon={<DeleteOutlined />} disabled={busy} />
              </Popconfirm>,
            ]}
          >
            <List.Item.Meta
              title={<span>{provider.name}</span>}
              description={
                <>
                  <span>{protocolName(provider.protocol)}</span>
                  <span> · {provider.model}</span>
                </>
              }
            />
            {payload.settings.defaultModelProvider === provider.id ? (
              <span className="default-resource-label">默认</span>
            ) : (
              <Button
                type="text"
                size="small"
                disabled={busy}
                aria-label={`设 ${provider.name} 为默认 AI 服务`}
                onClick={() => onSetDefault(provider.id)}
              >
                设为默认
              </Button>
            )}
          </List.Item>
        )}
      />
    </section>
  );
}

function ProviderEditor({ provider, busy, onBack, onSave }) {
  const [draft, setDraft] = useState(provider);
  const isDeepSeekPreset = !draft.id && draft.name === 'DeepSeek';
  const update = (field, value) => setDraft((current) => ({ ...current, [field]: value }));
  const useDeepSeekPreset = () =>
    setDraft({
      ...draft,
      name: 'DeepSeek',
      protocol: 'openai-chat-completions',
      baseUrl: 'https://api.deepseek.com',
      model: 'deepseek-v4-flash',
    });
  const canSave =
    draft.name.trim() &&
    draft.protocol &&
    draft.baseUrl.trim() &&
    draft.model.trim() &&
    (draft.id || draft.apiKey.trim());
  return (
    <section className="provider-editor">
      <div className="provider-editor-heading">
        <Button type="text" size="small" icon={<ArrowLeftOutlined />} onClick={onBack} disabled={busy}>
          返回服务列表
        </Button>
        <div>
          <Text>{draft.id ? '编辑 AI 服务' : '添加 AI 服务'}</Text>
          <span>选择协议后填写服务地址、模型和 API Key。</span>
        </div>
      </div>
      {!draft.id && (
        <Button
          className="deepseek-preset"
          size="small"
          onClick={useDeepSeekPreset}
          disabled={busy || isDeepSeekPreset}
        >
          使用 DeepSeek 预设
        </Button>
      )}
      <Form layout="vertical" className="provider-editor-form">
        <Form.Item label="服务名称">
          <Input
            value={draft.name}
            maxLength={48}
            placeholder="例如：我的 OpenAI 服务"
            onChange={(event) => update('name', event.target.value)}
          />
        </Form.Item>
        <Form.Item label="API 协议">
          <Select
            value={draft.protocol}
            options={protocols}
            onChange={(value) => update('protocol', value)}
          />
        </Form.Item>
        <Form.Item label="服务地址">
          <Input
            value={draft.baseUrl}
            placeholder="https://api.example.com"
            onChange={(event) => update('baseUrl', event.target.value)}
          />
        </Form.Item>
        <Form.Item label="模型名称">
          <Input
            value={draft.model}
            placeholder="填写服务提供方给出的模型 ID"
            onChange={(event) => update('model', event.target.value)}
          />
        </Form.Item>
        <Form.Item label="API Key">
          <Input.Password
            value={draft.apiKey}
            placeholder={draft.id ? '留空保留当前 API Key' : '仅保存到 Windows 凭据管理器'}
            onChange={(event) => update('apiKey', event.target.value)}
          />
        </Form.Item>
      </Form>
      <ActionBar>
        <Button disabled={busy} onClick={onBack}>
          取消
        </Button>
        <Button type="primary" loading={busy} disabled={!canSave} onClick={() => onSave(draft)}>
          {draft.id ? '保存修改' : '添加服务'}
        </Button>
      </ActionBar>
    </section>
  );
}

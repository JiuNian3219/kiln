import { Button, Empty, List, Popconfirm, Typography } from 'antd';
import { DeleteOutlined, InboxOutlined } from '@ant-design/icons';
import './CatalogPanel.css';

const { Text } = Typography;

export function CatalogPanel({
  label,
  items,
  busy,
  onImport,
  onDelete,
  onGenerateKnowledgeBaseIndex,
  onSpecifyKnowledgeBaseIndex,
}) {
  const isKnowledgeBase = label === '知识库';
  return (
    <section className="catalog-panel">
      <div className="catalog-heading">
        <Text>{label}</Text>
        <Button size="small" icon={<InboxOutlined />} onClick={onImport} disabled={busy}>
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
              ...(isKnowledgeBase
                ? [
                    <Button
                      key="generate-index"
                      size="small"
                      disabled={busy}
                      onClick={() => onGenerateKnowledgeBaseIndex(item.id)}
                    >
                      AI 索引
                    </Button>,
                    <Button
                      key="manual-index"
                      size="small"
                      disabled={busy}
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
                onConfirm={() => onDelete(item.id)}
              >
                <Button danger type="text" size="small" icon={<DeleteOutlined />} disabled={busy} />
              </Popconfirm>,
            ]}
          >
            <List.Item.Meta title={item.name} description={item.indexStatus || '已导入本地资料库'} />
          </List.Item>
        )}
      />
    </section>
  );
}

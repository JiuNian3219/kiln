import { Button, Empty, Popconfirm, Typography } from 'antd';
import './DiagnosticsPanel.css';

const { Text } = Typography;

export function DiagnosticsPanel({ diagnostics, busy, onRefresh, onCopy, onClear }) {
  const logEvents = diagnostics?.recentEvents || [];
  const logTime = (milliseconds) => new Date(milliseconds).toLocaleString('zh-CN', { hour12: false });
  return (
    <section className="diagnostics-panel">
      <div className="diagnostics-intro">
        <Text>本地诊断日志</Text>
        <span>
          仅保存在本机，最长保留 {diagnostics?.retentionDays ?? 14} 天；不含草稿、资料正文、检索词、目录路径或
          API Key。
        </span>
      </div>
      <div className="diagnostics-actions">
        <Button size="small" onClick={onRefresh} loading={busy}>
          刷新
        </Button>
        <Button size="small" onClick={onCopy} disabled={!diagnostics?.report}>
          复制诊断摘要
        </Button>
        <Popconfirm
          title="清除本地诊断日志？"
          description="此操作会删除当前设备上的排障记录。"
          okText="清除"
          cancelText="取消"
          onConfirm={onClear}
        >
          <Button size="small" danger disabled={busy}>
            清除日志
          </Button>
        </Popconfirm>
      </div>
      <div className="diagnostics-list">
        {logEvents.length ? (
          logEvents
            .slice()
            .reverse()
            .map((event, index) => (
              <article className={`diagnostic-event ${event.level}`} key={`${event.timestampMs}-${index}`}>
                <time>{logTime(event.timestampMs)}</time>
                <div>
                  <strong>{event.event}</strong>
                  <span>
                    {event.errorCode || '正常'} · {event.sessionId || '应用级事件'}
                  </span>
                </div>
              </article>
            ))
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂时没有诊断记录" />
        )}
      </div>
    </section>
  );
}

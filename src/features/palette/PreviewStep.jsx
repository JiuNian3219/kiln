import { Button } from 'antd';
import { CheckOutlined, ReloadOutlined } from '@ant-design/icons';
import { ActionBar } from '../../components/ActionBar';
import { PreviewBlock } from '../../components/PreviewBlock';
import './PreviewStep.css';

export function PreviewStep({
  original,
  replacement,
  busy,
  onCancel,
  onViewDetails,
  onRegenerate,
  onAccept,
}) {
  return (
    <section className="palette-body preview-body">
      <div className="compare-grid">
        <PreviewBlock label="原文" text={original} />
        <PreviewBlock label="建议" text={replacement} accent />
      </div>
      <div className="key-hint">Enter 确认替换 · Tab 查看完整对比 · Esc 取消</div>
      <ActionBar>
        <Button onClick={onCancel}>取消</Button>
        <Button onClick={onViewDetails}>查看完整对比</Button>
        <Button icon={<ReloadOutlined />} onClick={onRegenerate} loading={busy}>
          重新生成
        </Button>
        <Button type="primary" icon={<CheckOutlined />} onClick={onAccept} loading={busy}>
          替换
        </Button>
      </ActionBar>
    </section>
  );
}

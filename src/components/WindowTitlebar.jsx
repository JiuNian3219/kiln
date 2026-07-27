import { Button } from 'antd';
import { CloseOutlined } from '@ant-design/icons';
import './WindowTitlebar.css';

export function WindowTitlebar({ label, children, onClose, closeLabel = '关闭' }) {
  return (
    <header className="palette-titlebar">
      <div className="palette-drag-region" data-tauri-drag-region>
        <span className="app-mark">{label}</span>
      </div>
      <div className="titlebar-actions" data-no-window-drag>
        {children}
        {onClose && (
          <Button
            type="text"
            size="small"
            icon={<CloseOutlined />}
            onClick={onClose}
            aria-label={closeLabel}
          />
        )}
      </div>
    </header>
  );
}

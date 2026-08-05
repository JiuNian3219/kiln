import { useEffect, useState } from 'react';
import { Switch, Tooltip } from 'antd';
import { QuestionCircleOutlined } from '@ant-design/icons';
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';
import './AutostartSetting.css';

export function AutostartSetting() {
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(true);
  const [notice, setNotice] = useState('');

  useEffect(() => {
    let active = true;
    isEnabled()
      .then((value) => {
        if (active) setEnabled(value);
      })
      .catch(() => {
        if (active) setNotice('无法读取开机自启状态。');
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const changeAutostart = async (value) => {
    setLoading(true);
    setNotice('');
    try {
      if (value) await enable();
      else await disable();
      setEnabled(await isEnabled());
      setNotice(value ? '已设置为登录 Windows 时自动启动。' : '已关闭开机自启。');
    } catch {
      setNotice('无法更新开机自启，请稍后重试。');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="autostart-setting">
      <div className="autostart-setting-row">
        <Switch
          checked={enabled}
          loading={loading}
          checkedChildren="开"
          unCheckedChildren="关"
          onChange={changeAutostart}
        />
        <label>
          开机自启{' '}
          <Tooltip title="启用后，当前用户登录 Windows 时应用会在后台启动，并保留在系统托盘。">
            <QuestionCircleOutlined />
          </Tooltip>
        </label>
      </div>
      {notice && <small aria-live="polite">{notice}</small>}
    </div>
  );
}

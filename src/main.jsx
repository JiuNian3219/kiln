import React from 'react';
import { createRoot } from 'react-dom/client';
import { ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import App from './App.jsx';
import './styles.css';

createRoot(document.getElementById('root')).render(
  <React.StrictMode>
    <ConfigProvider locale={zhCN} theme={{
      token: {
        colorPrimary: '#78967d', colorInfo: '#78967d', colorBgBase: '#f5f6f2', colorBgContainer: '#fffefa',
        colorBgElevated: '#fffefa', colorTextBase: '#334039', colorBorder: '#dfe4dc', borderRadius: 6,
        fontFamily: 'Segoe UI Variable, Segoe UI, system-ui, sans-serif',
      },
    }}>
      <App />
    </ConfigProvider>
  </React.StrictMode>,
);

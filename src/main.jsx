import React from 'react';
import { createRoot } from 'react-dom/client';
import { ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import App from './App.jsx';
import './styles.css';

createRoot(document.getElementById('root')).render(
  <React.StrictMode>
    <ConfigProvider
      locale={zhCN}
      theme={{
        token: {
          colorPrimary: '#6f9078',
          colorInfo: '#6e91a5',
          colorSuccess: '#5d8a68',
          colorWarning: '#b4813f',
          colorError: '#b65e55',
          colorBgBase: '#f4f5f1',
          colorBgContainer: '#fcfcf9',
          colorBgElevated: '#ffffff',
          colorTextBase: '#29352e',
          colorTextSecondary: '#647168',
          colorBorder: '#d9ded6',
          borderRadius: 4,
          fontSize: 13,
          controlHeight: 32,
          fontFamily: 'Microsoft YaHei UI, Segoe UI Variable, Segoe UI, system-ui, sans-serif',
        },
      }}
    >
      <App />
    </ConfigProvider>
  </React.StrictMode>,
);

import { Component, StrictMode, type ErrorInfo, type ReactNode } from 'react';
import ReactDOM from 'react-dom/client';
import './styles.css';

function formatError(error: unknown) {
  if (error instanceof Error) {
    return error.stack ?? `${error.name}: ${error.message}`;
  }
  return String(error);
}

function BootErrorView(props: { title: string; error: string }) {
  return (
    <div className="min-h-screen bg-[linear-gradient(180deg,#f8fafc_0%,#f4f7fb_100%)] px-6 py-8 text-slate-900">
      <div className="mx-auto max-w-5xl rounded-[28px] border border-rose-200 bg-white p-6 shadow-[0_20px_60px_rgba(244,63,94,0.08)]">
        <div className="text-sm font-semibold uppercase tracking-[0.24em] text-rose-500">AirSlate Desktop Fatal Error</div>
        <h1 className="mt-3 text-2xl font-semibold tracking-[-0.04em] text-slate-950">{props.title}</h1>
        <pre className="mt-5 overflow-x-auto whitespace-pre-wrap break-words rounded-[20px] border border-rose-100 bg-rose-50 p-4 text-sm leading-7 text-rose-900">
          {props.error}
        </pre>
      </div>
    </div>
  );
}

class AppErrorBoundary extends Component<{ children: ReactNode }, { error: string | null }> {
  state = { error: null };

  static getDerivedStateFromError(error: unknown) {
    return { error: formatError(error) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    this.setState({ error: `${formatError(error)}\n\n${info.componentStack}` });
  }

  render() {
    if (this.state.error) {
      return <BootErrorView title="首屏渲染失败" error={this.state.error} />;
    }

    return this.props.children;
  }
}

const rootElement = document.getElementById('root');

if (!rootElement) {
  throw new Error('missing root element');
}

const root = ReactDOM.createRoot(rootElement);

function renderFatal(title: string, error: unknown) {
  root.render(<BootErrorView title={title} error={formatError(error)} />);
}

window.addEventListener('error', (event) => {
  renderFatal('启动阶段发生脚本错误', event.error ?? event.message);
});

window.addEventListener('unhandledrejection', (event) => {
  renderFatal('启动阶段发生未处理 Promise 错误', event.reason);
});

import('./App')
  .then(({ App }) => {
    root.render(
      <StrictMode>
        <AppErrorBoundary>
          <App />
        </AppErrorBoundary>
      </StrictMode>,
    );
  })
  .catch((error) => {
    renderFatal('前端模块加载失败', error);
  });

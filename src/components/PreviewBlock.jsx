import './PreviewBlock.css';

const preview = (text, limit = 240) => (text.length > limit ? `${text.slice(0, limit).trimEnd()}…` : text);

export function PreviewBlock({ label, text, accent }) {
  return (
    <div className={accent ? 'preview-block accent' : 'preview-block'}>
      <label>{label}</label>
      <pre>{preview(text, 260)}</pre>
    </div>
  );
}

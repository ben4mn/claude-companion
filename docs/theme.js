// Theme toggle — dark (default) vs warm (Claude cream).
// Priority: ?theme= query param → localStorage → system prefers-color-scheme → dark.

const KEY = 'cc.theme';
const root = document.documentElement;

function getInitial() {
  const params = new URLSearchParams(location.search);
  const fromUrl = params.get('theme');
  if (fromUrl === 'warm' || fromUrl === 'dark') return fromUrl;
  const saved = localStorage.getItem(KEY);
  if (saved === 'warm' || saved === 'dark') return saved;
  return 'dark';
}

function apply(theme) {
  root.setAttribute('data-theme', theme);
  const btn = document.getElementById('theme-toggle');
  if (btn) {
    btn.textContent = theme === 'warm' ? '🌙 Dark mode' : '☀︎ Warm mode';
    btn.setAttribute('aria-label', `Switch to ${theme === 'warm' ? 'dark' : 'warm'} mode`);
  }
}

function toggle() {
  const current = root.getAttribute('data-theme') === 'warm' ? 'warm' : 'dark';
  const next = current === 'warm' ? 'dark' : 'warm';
  localStorage.setItem(KEY, next);
  apply(next);
}

apply(getInitial());

document.addEventListener('DOMContentLoaded', () => {
  apply(root.getAttribute('data-theme') || getInitial());
  const btn = document.getElementById('theme-toggle');
  if (btn) btn.addEventListener('click', toggle);
});

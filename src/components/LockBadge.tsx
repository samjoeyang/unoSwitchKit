import { gateFrontend } from "../authGate";
import { useAuth } from "../authStore";

/// 登录层功能的小锁标记。放在需要登录的按钮/卡片旁，
/// 未登录时点击会引导到设置里的登录入口（真正的权限校验在 Rust 侧）。
export function LockBadge({ className = "" }: { className?: string }) {
  const auth = useAuth();
  if (auth.logged_in) return null;
  return (
    <button
      title="登录后可用"
      onClick={(e) => {
        e.stopPropagation();
        gateFrontend();
      }}
      className={"shrink-0 leading-none text-amber-300/80 hover:text-amber-200 " + className}
    >
      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-label="登录后可用">
        <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
        <path d="M7 11V7a5 5 0 0 1 10 0v4" />
      </svg>
    </button>
  );
}

/// 包装按钮的点击：未登录时引导登录并返回 false，调用方据此提前 return。
export function withGate(fn: () => void): () => void {
  return () => {
    if (!gateFrontend()) return;
    fn();
  };
}

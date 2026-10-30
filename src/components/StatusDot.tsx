export function StatusDot({ on }: { on: boolean }) {
  return (
    <span
      className={`inline-block h-2 w-2 rounded-full transition-colors ${
        on
          ? "bg-emerald-400 shadow-[0_0_6px_#34d399]"
          : "bg-zinc-500/60"
      }`}
    />
  );
}

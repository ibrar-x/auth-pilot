import type { UsageInfo } from "../types";

interface UsageBarProps {
  usage?: UsageInfo;
  loading?: boolean;
}

function formatResetTime(resetAt: number | null | undefined): string {
  if (!resetAt) return "";
  const now = Math.floor(Date.now() / 1000);
  const diff = resetAt - now;
  if (diff <= 0) return "now";
  if (diff < 60) return `${diff}s`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m`;
  return `${Math.floor(diff / 3600)}h ${Math.floor((diff % 3600) / 60)}m`;
}

function formatWindowDuration(minutes: number | null | undefined): string {
  if (!minutes) return "";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

function RateLimitBar({
  label,
  usedPercent,
  windowMinutes,
  resetsAt,
}: {
  label: string;
  usedPercent: number;
  windowMinutes?: number | null;
  resetsAt?: number | null;
}) {
  const clampedUsed = Math.min(Math.max(usedPercent, 0), 100);
  const remaining = Math.max(0, 100 - clampedUsed);

  const colorClass =
    clampedUsed < 70
      ? "bg-[#10b981]"
      : clampedUsed <= 90
        ? "bg-[#F37338]"
        : "bg-[#CF4500]";

  const windowLabel = formatWindowDuration(windowMinutes);
  const resetLabel = formatResetTime(resetsAt);

  return (
    <div className="space-y-1.5">
      <div className="flex justify-between text-xs">
        <span className="text-[#696969] dark:text-[#9a9a9a]">
          {label} {windowLabel && <span className="text-[#D1CDC7] dark:text-[#696969]">({windowLabel})</span>}
        </span>
        <span className="text-[#141413] dark:text-[#f3f0ee] font-medium">
          {remaining.toFixed(0)}% remaining
        </span>
      </div>
      <div className="h-2 bg-[#F3F0EE] dark:bg-[#2a2a2a] rounded-[4px] overflow-hidden">
        <div
          className={`h-full transition-all duration-500 ${colorClass}`}
          style={{ width: `${clampedUsed}%` }}
        ></div>
      </div>
      <div className="flex justify-between text-[10px] text-[#D1CDC7] dark:text-[#696969]">
        <span>{clampedUsed.toFixed(0)}% used</span>
        {resetLabel && <span>Resets in {resetLabel}</span>}
      </div>
    </div>
  );
}

export function UsageBar({ usage, loading }: UsageBarProps) {
  if (loading && !usage) {
    return (
      <div className="space-y-2">
        <div className="text-xs text-[#D1CDC7] dark:text-[#696969] italic animate-pulse">
          Fetching usage...
        </div>
        <div className="h-2 bg-[#F3F0EE] dark:bg-[#2a2a2a] rounded-[4px] overflow-hidden animate-pulse">
          <div className="h-full w-2/3 bg-[#D1CDC7] dark:bg-[#3a3a3a]"></div>
        </div>
      </div>
    );
  }

  if (!usage) {
    return (
      <div className="text-xs text-[#D1CDC7] dark:text-[#696969] italic py-1 animate-pulse">
        Fetching usage...
      </div>
    );
  }

  if (usage.error) {
    return (
      <div className="text-xs text-[#D1CDC7] dark:text-[#696969] italic py-1">
        {usage.error}
      </div>
    );
  }

  const hasPrimary = usage.primary_used_percent !== null && usage.primary_used_percent !== undefined;
  const hasSecondary = usage.secondary_used_percent !== null && usage.secondary_used_percent !== undefined;

  if (!hasPrimary && !hasSecondary) {
    return (
      <div className="text-xs text-[#D1CDC7] dark:text-[#696969] italic py-1">
        No rate limit data
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {hasPrimary && (
        <RateLimitBar
          label="5-Hour Window"
          usedPercent={usage.primary_used_percent!}
          windowMinutes={usage.primary_window_minutes}
          resetsAt={usage.primary_resets_at}
        />
      )}
      {hasSecondary && (
        <RateLimitBar
          label="7-Day Window"
          usedPercent={usage.secondary_used_percent!}
          windowMinutes={usage.secondary_window_minutes}
          resetsAt={usage.secondary_resets_at}
        />
      )}
      {usage.credits_balance && (
        <div className="text-xs text-[#696969] dark:text-[#9a9a9a]">
          Credits: {usage.credits_balance}
        </div>
      )}
    </div>
  );
}

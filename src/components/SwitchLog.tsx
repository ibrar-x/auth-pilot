import type { SwitchEvent } from "../types";

interface SwitchLogProps {
  events: SwitchEvent[];
  onClose: () => void;
}

function formatTime(timestamp: string): string {
  const date = new Date(timestamp);
  return date.toLocaleString();
}

function formatShortTime(timestamp?: string | null): string {
  if (!timestamp) {
    return "not recorded";
  }
  return new Date(timestamp).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function getReasonLabel(reason: SwitchEvent["reason"]): string {
  switch (reason) {
    case "auto_limit_reached":
      return "Auto — Limit Reached";
    case "auto_depleted":
      return "Auto — All Depleted";
    case "manual":
      return "Manual";
    default:
      return "Unknown";
  }
}

function getRestartSummary(event: SwitchEvent): { title: string; detail: string } {
  if (!event.codex_was_running) {
    return {
      title: "Codex was closed",
      detail: "No restart was needed",
    };
  }

  const stoppedAt = formatShortTime(event.codex_stopped_at);
  const restartedAt = event.codex_restarted_at
    ? formatShortTime(event.codex_restarted_at)
    : "not reopened";

  let resume = "resume off";
  if (!event.recovery_session_id) {
    resume = "no session captured";
  } else if (event.auto_resume_attempted && event.auto_resume_started) {
    resume = "resume started";
  } else if (event.auto_resume_attempted) {
    resume = "resume failed";
  }

  const session = event.recovery_session_id
    ? ` · session ${event.recovery_session_id.slice(0, 8)}`
    : "";

  return {
    title: "Codex restarted",
    detail: `Stopped ${stoppedAt} · reopened ${restartedAt} · ${resume}${session}`,
  };
}

export function SwitchLog({ events, onClose }: SwitchLogProps) {
  return (
    <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-[#1f1f1f] w-full max-w-4xl mx-4 rounded-[4px] shadow-l2 max-h-[80vh] overflow-y-auto animate-fade-in-up">
        <div className="flex items-center justify-between p-6 pb-4">
          <h2 className="text-lg font-medium text-[#141413] dark:text-[#f3f0ee]">Switch Log</h2>
          <button
            onClick={onClose}
            className="h-8 w-8 flex items-center justify-center rounded-[4px] text-[#696969] hover:text-[#141413] dark:hover:text-[#f3f0ee] hover:bg-[#F3F0EE] dark:hover:bg-[#2a2a2a] transition-colors"
          >
            ✕
          </button>
        </div>

        <div className="px-6 pb-6">
          {events.length === 0 ? (
            <div className="text-center py-8 text-[#D1CDC7] dark:text-[#696969]">
              No switch events yet
            </div>
          ) : (
            <table className="w-full">
              <thead>
                <tr className="border-b border-[#F3F0EE] dark:border-[#2a2a2a]">
                  <th className="text-left text-xs font-medium text-[#696969] dark:text-[#9a9a9a] uppercase tracking-wider py-2">
                    Time
                  </th>
                  <th className="text-left text-xs font-medium text-[#696969] dark:text-[#9a9a9a] uppercase tracking-wider py-2">
                    From → To
                  </th>
                  <th className="text-left text-xs font-medium text-[#696969] dark:text-[#9a9a9a] uppercase tracking-wider py-2">
                    Reason
                  </th>
                  <th className="text-left text-xs font-medium text-[#696969] dark:text-[#9a9a9a] uppercase tracking-wider py-2">
                    Codex Restart
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[#F3F0EE] dark:divide-[#2a2a2a]">
                {events.map((event, index) => {
                  const restart = getRestartSummary(event);

                  return (
                    <tr key={index} className="hover:bg-[#FCFBFA] dark:hover:bg-[#2a2a2a]/50 transition-colors">
                      <td className="py-3 text-sm text-[#696969] dark:text-[#9a9a9a]">
                        {formatTime(event.timestamp)}
                      </td>
                      <td className="py-3 text-sm text-[#141413] dark:text-[#f3f0ee]">
                        {event.from_account_id
                          ? `${event.from_account_id.slice(0, 8)}...`
                          : "—"}
                        {" → "}
                        {event.to_account_id.slice(0, 8)}...
                      </td>
                      <td className="py-3">
                        <span
                          className={`inline-flex px-3 py-1 text-xs font-medium rounded-[4px] ${
                            event.reason === "manual"
                              ? "bg-[#F3F0EE] text-[#141413] dark:bg-[#2a2a2a] dark:text-[#f3f0ee]"
                              : event.reason === "auto_limit_reached"
                              ? "bg-[#F3F0EE] text-[#F37338] dark:bg-[#2a2a2a] dark:text-[#F37338]"
                              : "bg-[#F3F0EE] text-[#CF4500] dark:bg-[#2a2a2a] dark:text-[#CF4500]"
                          }`}
                        >
                          {getReasonLabel(event.reason)}
                        </span>
                      </td>
                      <td className="py-3 text-sm">
                        <div className="font-medium text-[#141413] dark:text-[#f3f0ee]">
                          {restart.title}
                        </div>
                        <div className="mt-1 max-w-[320px] text-xs leading-5 text-[#696969] dark:text-[#9a9a9a]">
                          {restart.detail}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}

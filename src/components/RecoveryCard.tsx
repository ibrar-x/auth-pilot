import { useMemo, useState } from "react";
import type React from "react";
import type { CodexSession } from "../types";
import { RecoveryDialog } from "./RecoveryDialog";

interface RecoveryCardProps {
  session: CodexSession;
  index: number;
  total: number;
  resumeAvailable: boolean;
  colors: {
    text: string;
    mutedStrong: string;
    muted: string;
    faint: string;
    hairline: string;
    rowHover: string;
    footerHover: string;
    border: string;
  };
  onPrevious: () => void;
  onNext: () => void;
  onReopen: () => void;
  onCopyPrompt: () => void;
  onResume: () => void;
  onIgnore: () => void;
}

export function RecoveryCard({
  session,
  index,
  total,
  resumeAvailable,
  colors,
  onPrevious,
  onNext,
  onReopen,
  onCopyPrompt,
  onResume,
  onIgnore,
}: RecoveryCardProps) {
  const [expanded, setExpanded] = useState(false);
  const [confirmingResume, setConfirmingResume] = useState(false);
  const title = total > 1 ? `Session interrupted ${index + 1} of ${total}` : "Session interrupted";
  const workspace = useMemo(() => compactPath(session.workspace_path), [session.workspace_path]);

  return (
    <div style={{ margin: "10px 10px 8px", padding: "10px", borderRadius: 6, border: `0.5px solid ${colors.border}`, background: "rgba(248,113,113,0.09)", flexShrink: 0 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
        <span style={{ width: 7, height: 7, borderRadius: "50%", background: "#f87171", flexShrink: 0 }} />
        <div style={{ minWidth: 0, flex: 1, color: colors.text, fontSize: 11, fontWeight: 700, letterSpacing: "0.01em" }}>{title}</div>
        {total > 1 && (
          <div style={{ display: "flex", gap: 2, flexShrink: 0 }}>
            <IconButton colors={colors} label="Previous interrupted session" onClick={onPrevious}>‹</IconButton>
            <IconButton colors={colors} label="Next interrupted session" onClick={onNext}>›</IconButton>
          </div>
        )}
      </div>

      <div title={session.workspace_path} style={{ color: colors.mutedStrong, fontSize: 10, lineHeight: 1.35, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", marginBottom: 8 }}>
        {workspace || "Workspace unavailable"}
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 5 }}>
        <ActionButton colors={colors} label="Reopen" onClick={onReopen} />
        <ActionButton colors={colors} label="Copy prompt" onClick={onCopyPrompt} />
        <ActionButton colors={colors} label="Ignore" onClick={onIgnore} subdued />
        <ActionButton colors={colors} label={expanded ? "Less" : "Advanced"} onClick={() => setExpanded((value) => !value)} subdued />
      </div>

      {expanded && (
        <div style={{ marginTop: 8, paddingTop: 8, borderTop: `0.5px solid ${colors.hairline}` }}>
          <div style={{ color: colors.muted, fontSize: 9.5, lineHeight: 1.35, marginBottom: 7 }}>
            Background resume runs Codex through the CLI and writes output to a recovery log.
          </div>
          {resumeAvailable ? (
            <ActionButton colors={colors} label="Resume in background" onClick={() => setConfirmingResume(true)} />
          ) : (
            <div style={{ color: colors.faint, fontSize: 9.5 }}>Background resume unavailable for this Codex CLI.</div>
          )}
          {confirmingResume && (
            <RecoveryDialog
              colors={colors}
              onCancel={() => setConfirmingResume(false)}
              onConfirm={() => {
                setConfirmingResume(false);
                onResume();
              }}
            />
          )}
        </div>
      )}
    </div>
  );
}

function ActionButton({ colors, label, onClick, subdued }: { colors: RecoveryCardProps["colors"]; label: string; onClick: () => void; subdued?: boolean }) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      onPointerDown={(event) => event.stopPropagation()}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      style={{ height: 25, borderRadius: 4, border: `0.5px solid ${colors.border}`, background: hovered ? colors.footerHover : subdued ? "transparent" : "rgba(255,255,255,0.04)", color: subdued ? colors.mutedStrong : colors.text, fontSize: 10, fontWeight: 600, cursor: "pointer", padding: "0 6px", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}
    >
      {label}
    </button>
  );
}

function IconButton({ colors, label, onClick, children }: { colors: RecoveryCardProps["colors"]; label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      title={label}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      style={{ width: 20, height: 20, borderRadius: 4, border: `0.5px solid ${colors.border}`, background: "transparent", color: colors.mutedStrong, cursor: "pointer", fontSize: 14, lineHeight: "18px", padding: 0 }}
    >
      {children}
    </button>
  );
}

function compactPath(path: string): string {
  if (!path) return "";
  const parts = path.split("/").filter(Boolean);
  if (parts.length <= 3) return path;
  return `…/${parts.slice(-3).join("/")}`;
}

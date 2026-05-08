interface RecoveryDialogProps {
  colors: {
    text: string;
    muted: string;
    mutedStrong: string;
    border: string;
    footerHover: string;
  };
  onCancel: () => void;
  onConfirm: () => void;
}

export function RecoveryDialog({ colors, onCancel, onConfirm }: RecoveryDialogProps) {
  return (
    <div style={{ marginTop: 8, padding: 8, borderRadius: 6, border: `0.5px solid ${colors.border}`, background: "rgba(0,0,0,0.12)" }}>
      <div style={{ color: colors.text, fontSize: 10.5, fontWeight: 700, marginBottom: 5 }}>
        Resume in background?
      </div>
      <div style={{ color: colors.muted, fontSize: 9.5, lineHeight: 1.35, marginBottom: 8 }}>
        Codex will continue through the CLI. Output is written to a recovery log instead of live desktop output.
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6 }}>
        <DialogButton colors={colors} label="Cancel" onClick={onCancel} subdued />
        <DialogButton colors={colors} label="Resume" onClick={onConfirm} />
      </div>
    </div>
  );
}

function DialogButton({
  colors,
  label,
  onClick,
  subdued,
}: {
  colors: RecoveryDialogProps["colors"];
  label: string;
  onClick: () => void;
  subdued?: boolean;
}) {
  return (
    <button
      type="button"
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      style={{ height: 24, borderRadius: 4, border: `0.5px solid ${colors.border}`, background: subdued ? "transparent" : colors.footerHover, color: subdued ? colors.mutedStrong : colors.text, cursor: "pointer", fontSize: 10, fontWeight: 700 }}
    >
      {label}
    </button>
  );
}

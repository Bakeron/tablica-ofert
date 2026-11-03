import { useI18n } from "../i18n";
import { IconWarning } from "./Icons";
import { CloseButton, Modal } from "./Modal";

export function ConfirmDialog({
  message,
  confirmLabel,
  onConfirm,
  onClose,
}: {
  message: string | null;
  confirmLabel: string;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  return (
    <Modal open={message !== null} onClose={onClose} labelledBy="confirm-title">
      <div className="sheet sheet-narrow">
        <header className="sheet-head">
          <h2 id="confirm-title" className="warn-title">
            <IconWarning /> {t("error.title")}
          </h2>
          <CloseButton onClick={onClose} label={t("add.cancel")} />
        </header>
        <p>{message}</p>
        <footer className="sheet-foot">
          <button type="button" className="btn btn-ghost" onClick={onClose} data-autofocus>
            {t("add.cancel")}
          </button>
          <button
            type="button"
            className="btn btn-danger"
            onClick={() => {
              onConfirm();
              onClose();
            }}
          >
            {confirmLabel}
          </button>
        </footer>
      </div>
    </Modal>
  );
}

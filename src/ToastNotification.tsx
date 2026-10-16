import { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import logoSvg from "./assets/logo.svg";

export default function ToastNotification() {
  const [closing, setClosing] = useState(false);
  const [customMsg, setCustomMsg] = useState<string | null>(null);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    document.documentElement.classList.add("is-toast");
    document.body.classList.add("is-toast");

    const unlistenPromise = listen<string>("toast_message", (event) => {
      if (event.payload) {
        setCustomMsg(event.payload);
      }
    });

    // Auto-dismiss after 5.5 seconds
    timerRef.current = setTimeout(() => {
      handleDismiss();
    }, 5500);

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const handleDismiss = () => {
    setClosing(true);
    setTimeout(() => {
      invoke("hide_toast_notification").catch(() => {});
    }, 320);
  };

  const handleOpenMain = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest(".toast-close-btn")) {
      return;
    }
    invoke("open_main_from_toast").catch(() => {});
  };

  const isUpdateNotice = customMsg?.toLowerCase().includes("güncelle") ?? false;

  return (
    <div className="toast-wrapper" onClick={handleOpenMain}>
      <div className={`toast-card ${closing ? "closing" : "opening"}`}>
        <div className="toast-left">
          <div className="toast-logo-box">
            <img src={logoSvg} alt="Slipstream" className="toast-logo" />
            <span className="toast-status-beacon" />
          </div>
        </div>

        <div className="toast-center">
          <div className="toast-title-row">
            <span className="toast-app-name">Slipstream</span>
            <span className={`toast-badge ${isUpdateNotice ? "update" : ""}`}>
              {isUpdateNotice ? "⚡ GÜNCELLEME" : "ARKA PLANDA AKTİF"}
            </span>
          </div>
          <p className="toast-body-text">
            {customMsg || "Ağ koruması devrede. Sistem tepsisinden dilediğiniz zaman açabilirsiniz."}
          </p>
        </div>

        <div className="toast-actions">
          <button
            className="toast-open-btn"
            title="Aç"
            onClick={handleOpenMain}
          >
            Aç
          </button>
          <button
            className="toast-close-btn"
            title="Bildirimi Kapat"
            onClick={(e) => {
              e.stopPropagation();
              handleDismiss();
            }}
          >
            ✕
          </button>
        </div>

        <div className="toast-progress-track">
          <div className="toast-progress-fill" />
        </div>
      </div>
    </div>
  );
}

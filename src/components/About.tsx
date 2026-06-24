import { useStore } from "../lib/store";
import { X, Github } from "lucide-react";
import { open as openExternal } from "@tauri-apps/plugin-shell";

declare const __APP_VERSION__: string;

// Fallback if no Vite define is configured.
const VERSION =
  typeof __APP_VERSION__ !== "undefined" ? __APP_VERSION__ : "0.1.1";

const GITHUB_URL = "https://github.com/NousResearch/Skyhook";

export function About() {
  const close = useStore((s) => s.closeAbout);

  const openGithub = () => {
    openExternal(GITHUB_URL).catch((e) => console.error("open github", e));
  };

  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <div className="modal-title">About</div>
          <button className="btn btn-ghost btn-icon" onClick={close}>
            <X size={16} />
          </button>
        </div>
        <div className="about-body">
          <div className="brand-mark">S</div>
          <div className="about-name">Skyhook</div>
          <div className="about-meta">Version {VERSION}</div>
          <div className="about-tagline">
            A modern SFTP client — fast, keyboard-first, and built for people
            who live in remote filesystems.
          </div>
          <button className="about-link" onClick={openGithub}>
            <Github
              size={14}
              style={{ verticalAlign: "-2px", marginRight: 6 }}
            />
            View on GitHub
          </button>
          <div className="about-meta">Released under the MIT License.</div>
        </div>
        <div className="modal-footer">
          <button className="btn btn-primary" onClick={close}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

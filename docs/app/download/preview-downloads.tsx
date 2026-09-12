"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { githubPreviewUrl, loadLatestPreview, type PreviewRelease } from "@/lib/release";
import { FileLink } from "./file-link";
import styles from "./page.module.css";

type PreviewState =
  | { status: "loading" | "unavailable" | "empty" }
  | { status: "ready"; release: PreviewRelease };

export function PreviewDownloads() {
  const [state, setState] = useState<PreviewState>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    let active = true;
    const timeout = window.setTimeout(() => controller.abort(), 10_000);
    loadLatestPreview(controller.signal)
      .then((release) => {
        if (active) setState(release ? { status: "ready", release } : { status: "empty" });
      })
      .catch(() => {
        if (active) setState({ status: "unavailable" });
      })
      .finally(() => window.clearTimeout(timeout));
    return () => {
      active = false;
      window.clearTimeout(timeout);
      controller.abort();
    };
  }, [attempt]);

  return (
    <section id="preview" className={styles.previewSection} aria-labelledby="preview-heading">
      <div className="page-shell">
        <div className={styles.previewIntro}>
          <div>
            <p className={styles.downloadKicker}>For early testing</p>
            <h2 id="preview-heading">Try a preview.</h2>
          </div>
          <div>
            <p>Get the latest fixes before they reach the app stores. Previews are built in Release mode and may have unfinished changes.</p>
            <p className={styles.previewLinks}>
              <a className="text-link" href={githubPreviewUrl} rel="noreferrer">Browse preview releases</a>
              <Link className="text-link" href="/guide/#previews">Installing and updating</Link>
            </p>
          </div>
        </div>

        {state.status === "ready" ? (
          <>
            <p className={styles.releaseIdentity}>
              <span>Latest preview publication</span>
              <strong>{state.release.version}-preview.{state.release.number}</strong>
              <time dateTime={state.release.publishedAt}>{new Date(state.release.publishedAt).toISOString().slice(0, 10)}</time>
            </p>
            <ul className={styles.downloadList}>
              <li>
                <div className={styles.platformName}><h3>macOS</h3><span>Apple Silicon · macOS 15+</span></div>
                <div className={styles.platformDetails}>
                  <p>Signed and notarized DMG. Replaces the installed direct app; future previews are downloaded here.</p>
                  {state.release.dmg ? <FileLink asset={state.release.dmg} /> : <span>No preview download available.</span>}
                </div>
              </li>
              <li>
                <div className={styles.platformName}><h3>Windows</h3><span>x64 · Windows 10 2004+</span></div>
                <div className={styles.platformDetails}>
                  <p>Native WinUI installer. Unsigned; SmartScreen may warn when you run it.</p>
                  {state.release.windowsExe ? <FileLink asset={state.release.windowsExe} /> : <span>No preview download available.</span>}
                </div>
              </li>
              <li>
                <div className={styles.platformName}><h3>Linux</h3><span>x64 · GTK / libadwaita</span></div>
                <div className={styles.platformDetails}>
                  <p>DEB for Ubuntu 24.04 or newer. RPM built for Fedora 43. Replaces the installed VniDrop package.</p>
                  <p className={styles.downloadActions}>{state.release.deb ? <FileLink asset={state.release.deb} /> : <span>No preview download available.</span>}{state.release.rpm ? <FileLink asset={state.release.rpm} /> : <span>No preview download available.</span>}</p>
                </div>
              </li>
              <li>
                <div className={styles.platformName}><h3>Android</h3><span>Android 7+ · arm64 / x86_64</span></div>
                <div className={styles.platformDetails}>
                  <p>Signed with the preview key. Installs alongside the Play Store app with separate app data.</p>
                  {state.release.apk ? <FileLink asset={state.release.apk} /> : <span>No preview download available.</span>}
                </div>
              </li>
            </ul>
            <p className={styles.downloadChecksums}>
              <a className="text-link" href={state.release.checksumsUrl} rel="noreferrer">Preview SHA256 checksums</a>
              {" · "}<a className="text-link" href={state.release.tagUrl} rel="noreferrer">Release notes</a>
              {" · "}<a className="text-link" href={state.release.manifestUrl} rel="noreferrer">Build manifest</a>
            </p>
            <p className={styles.previewNote}>Desktop previews share your existing app data and keep the base product version. Moving between builds of the same version may require reinstalling. iOS previews are not included.</p>
          </>
        ) : (
          <div className={styles.previewStatus} role="status">
            {state.status === "loading" ? <p>Checking for a published preview…</p> : null}
            {state.status === "empty" ? <p>No preview found among recent releases. Check GitHub for older previews, or use the current release downloads above.</p> : null}
            {state.status === "unavailable" ? (
              <>
                <p>Preview downloads could not be loaded. You can still find them on GitHub.</p>
                <button className="text-link" onClick={() => { setState({ status: "loading" }); setAttempt((value) => value + 1); }}>Try again</button>
              </>
            ) : null}
          </div>
        )}
      </div>
    </section>
  );
}

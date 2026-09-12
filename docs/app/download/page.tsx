import type { Metadata } from "next";
import Link from "next/link";
import { CopyCommand } from "@/components/copy-command";
import {
  githubLatestUrl,
  githubRepoUrl,
  homebrewInstall,
  loadLatestRelease,
  windowsStoreUrl,
} from "@/lib/release";
import { FileLink } from "./file-link";
import { PreviewDownloads } from "./preview-downloads";
import styles from "./page.module.css";

export const metadata: Metadata = {
  title: "Download",
  description:
    "Download VniDrop for macOS, Windows, Linux, and Android. Choose the current release or a preview for early testing, with installation help and checksums.",
};

export default async function DownloadPage() {
  const release = await loadLatestRelease();
  const channelLabel = release.channel === "beta" ? "Beta" : release.channel;

  return (
    <main id="main-content" className={styles.downloadPage}>
      <section className={styles.downloadHero}>
        <div className={`${styles.downloadHeroInner} page-shell`}>
          <div>
            <p className={styles.downloadKicker}>Latest downloads by platform</p>
            <h1>Choose this device.</h1>
          </div>
          <div className={styles.releaseSummary}>
            <p className={styles.releaseIdentity}>
              <span>{channelLabel} · Latest publication</span>
              <strong>{release.tag}</strong>
            </p>
            <p>
              Public builds come from GitHub Releases. Windows is available through the Microsoft
              Store or as an unsigned direct installer. Platforms can receive updates separately; each download shows its version and checksums.
            </p>
            <p className={styles.channelLinks}><a className="text-link" href="#preview">Preview downloads ↓</a><Link className="text-link" href="/guide/">Installation and help</Link></p>
          </div>
        </div>
      </section>

      <section className={styles.downloadListSection}>
        <div className="page-shell">
          <ul className={styles.downloadList}>
            <li id="macos">
              <div className={styles.platformName}>
                <h2>macOS</h2>
                <span>Download</span>
              </div>
              <div className={styles.platformDetails}>
                <p>Signed and notarized disk image for Apple Silicon. The direct app checks for release updates after installation.</p>
                <p className={styles.downloadActions}>
                  {release.dmg ? <FileLink asset={release.dmg} /> : <span>No disk image available.</span>}
                </p>
                <div className={styles.installCommand}>
                  <CopyCommand command={homebrewInstall} />
                </div>
              </div>
            </li>
            <li id="linux">
              <div className={styles.platformName}>
                <h2>Linux</h2>
                <span>Packages</span>
              </div>
              <div className={styles.platformDetails}>
                <p>64-bit DEB and RPM packages. Check this release&apos;s notes for its supported distributions.</p>
                <p className={styles.downloadActions}>
                  {release.deb ? <FileLink asset={release.deb} /> : null}
                  {release.rpm ? <FileLink asset={release.rpm} /> : null}
                  {!release.deb && !release.rpm ? <span>No Linux packages available.</span> : null}
                </p>
              </div>
            </li>
            <li id="android">
              <div className={styles.platformName}>
                <h2>Android</h2>
                <span>Sideload</span>
              </div>
              <div className={styles.platformDetails}>
                <p>Play-signed APK for sideload. The Play listing is still in closed testing.</p>
                <p className={styles.downloadActions}>
                  {release.apk ? <FileLink asset={release.apk} /> : <span>No Android APK available.</span>}
                </p>
              </div>
            </li>
            <li id="windows">
              <div className={styles.platformName}>
                <h2>Windows</h2>
                <span>Store or direct</span>
              </div>
              <div className={styles.platformDetails}>
                <p>Choose the signed Microsoft Store build or download the unsigned 64-bit installer.</p>
                <p className={styles.downloadActions}>
                  <a className="text-link" href={windowsStoreUrl} rel="noreferrer">
                    Open Microsoft Store
                  </a>
                  {release.windowsExe ? (
                    <FileLink asset={release.windowsExe} />
                  ) : (
                    <span>No direct installer available.</span>
                  )}
                </p>
                {release.windowsExe ? (
                  <p className={styles.downloadWarning}>
                    The direct installer is unsigned, so SmartScreen may warn about its publisher.
                    Verify it against the installer&apos;s linked SHA256 checksums before running it.
                  </p>
                ) : null}
              </div>
            </li>
            <li id="ios">
              <div className={styles.platformName}>
                <h2>iOS</h2>
                <span>Source only</span>
              </div>
              <div className={styles.platformDetails}>
                <p>The native SwiftUI app can be built with Xcode. iOS is not included in direct preview downloads.</p>
                <p className={styles.downloadActions}>
                  <a className="text-link" href={`${githubRepoUrl}/blob/master/apple/README.md`} target="_blank" rel="noreferrer">
                    Apple build guide
                  </a>
                </p>
              </div>
            </li>
          </ul>

          <p className={styles.downloadChecksums}>
            <a className="text-link" href={release.checksumsUrl} rel="noreferrer">
              SHA256 checksums
            </a>
            {" · "}
            <a className="text-link" href={release.tagUrl} rel="noreferrer">
              GitHub {release.tag}
            </a>
            {" · "}
            <a className="text-link" href={githubLatestUrl} rel="noreferrer">
              Latest release
            </a>
          </p>
        </div>
      </section>
      <PreviewDownloads />
    </main>
  );
}

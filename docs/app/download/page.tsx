import type { Metadata } from "next";
import { CopyCommand } from "@/components/copy-command";
import {
  formatBytes,
  githubLatestUrl,
  githubRepoUrl,
  homebrewInstall,
  loadLatestRelease,
  windowsStoreUrl,
  type ReleaseAsset,
} from "@/lib/release";
import styles from "./page.module.css";

export const metadata: Metadata = {
  title: "Download",
  description:
    "Beta builds of VniDrop for macOS, Linux, Android, and Windows. Early development; iOS is not in a public store yet.",
};

function FileLink({ asset }: { asset: ReleaseAsset }) {
  return (
    <a className="file-link" href={asset.url} rel="noreferrer">
      {asset.name}
      <span className={styles.downloadMeta}> {formatBytes(asset.bytes)}</span>
    </a>
  );
}

export default async function DownloadPage() {
  const release = await loadLatestRelease();
  const channelLabel = release.channel === "beta" ? "Beta" : release.channel;

  return (
    <main id="main-content" className={styles.downloadPage}>
      <section className={styles.downloadHero}>
        <div className={`${styles.downloadHeroInner} page-shell`}>
          <div>
            <p className={styles.downloadKicker}>Current public release</p>
            <h1>Choose this device.</h1>
          </div>
          <div className={styles.releaseSummary}>
            <p className={styles.releaseIdentity}>
              <span>{channelLabel}</span>
              <strong>{release.tag}</strong>
            </p>
            <p>
              Public builds come from GitHub Releases. Windows installs through the Microsoft Store.
              iOS is available from source for now.
            </p>
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
                <p>Notarized disk image. The app can update itself after install.</p>
                <p className={styles.downloadActions}>
                  {release.dmg ? <FileLink asset={release.dmg} /> : <span>No disk image in this release.</span>}
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
                <p>64-bit packages for Debian/Ubuntu and Fedora/RHEL.</p>
                <p className={styles.downloadActions}>
                  {release.deb ? <FileLink asset={release.deb} /> : null}
                  {release.rpm ? <FileLink asset={release.rpm} /> : null}
                  {!release.deb && !release.rpm ? <span>No Linux packages in this release.</span> : null}
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
                  {release.apk ? <FileLink asset={release.apk} /> : <span>No Android APK in this release.</span>}
                </p>
              </div>
            </li>
            <li id="windows">
              <div className={styles.platformName}>
                <h2>Windows</h2>
                <span>Store</span>
              </div>
              <div className={styles.platformDetails}>
                <p>Install from the Microsoft Store. There is no public sideload package.</p>
                <p className={styles.downloadActions}>
                  <a className="text-link" href={windowsStoreUrl} rel="noreferrer">
                    Open Microsoft Store
                  </a>
                </p>
              </div>
            </li>
            <li id="ios">
              <div className={styles.platformName}>
                <h2>iOS</h2>
                <span>Source only</span>
              </div>
              <div className={styles.platformDetails}>
                <p>Native app, not in a public store yet. Build from source if you need it today.</p>
                <p className={styles.downloadActions}>
                  <a className="text-link" href={githubRepoUrl} target="_blank" rel="noreferrer">
                    View source on GitHub
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
    </main>
  );
}

import type { Metadata } from "next";
import {
  formatBytes,
  githubLatestUrl,
  githubRepoUrl,
  homebrewInstall,
  loadLatestRelease,
  windowsStoreUrl,
  type ReleaseAsset,
} from "@/lib/release";
import "./download.css";

export const metadata: Metadata = {
  title: "Download",
  description:
    "Beta builds of VniDrop for macOS, Linux, Android, and Windows. Early development; iOS is not in a public store yet.",
};

function FileLink({ asset, label }: { asset: ReleaseAsset; label: string }) {
  return (
    <a className="text-link" href={asset.url}>
      {label}
      <span className="download-meta">
        {" "}
        · {formatBytes(asset.bytes)}
      </span>
    </a>
  );
}

export default async function DownloadPage() {
  const release = await loadLatestRelease();
  const channelLabel = release.channel === "beta" ? "Beta" : release.channel;

  return (
    <main id="main-content" className="download-page">
      <section className="download-hero">
        <div className="page-shell download-hero-inner">
          <p className="hero-status">
            {channelLabel} · {release.tag}
          </p>
          <h1>
            Download VniDrop.
            <em>Early development.</em>
          </h1>
          <p>
            These are the current public builds. VniDrop is still changing. macOS, Linux, and
            Android ship as files from GitHub Releases. Windows is on the Microsoft Store. iOS is
            not in a public store yet.
          </p>
        </div>
      </section>

      <section className="download-list-section">
        <div className="page-shell">
          <ul className="download-list">
            <li>
              <h2>macOS</h2>
              <p>Notarized disk image. The app can update itself after install.</p>
              <p className="download-actions">
                {release.dmg ? (
                  <FileLink asset={release.dmg} label={release.dmg.name} />
                ) : (
                  <span>No disk image in this release.</span>
                )}
              </p>
              <pre>
                <code>{homebrewInstall}</code>
              </pre>
            </li>
            <li>
              <h2>Linux</h2>
              <p>64-bit packages for Debian/Ubuntu and Fedora/RHEL.</p>
              <p className="download-actions">
                {release.deb ? <FileLink asset={release.deb} label="Debian / Ubuntu .deb" /> : null}
                {release.rpm ? <FileLink asset={release.rpm} label="Fedora / RHEL .rpm" /> : null}
                {!release.deb && !release.rpm ? <span>No Linux packages in this release.</span> : null}
              </p>
            </li>
            <li>
              <h2>Android</h2>
              <p>
                Play-signed APK for sideload. The Play listing is still in closed testing, not
                production.
              </p>
              <p className="download-actions">
                {release.apk ? (
                  <FileLink asset={release.apk} label={release.apk.name} />
                ) : (
                  <span>No Android APK in this release.</span>
                )}
              </p>
            </li>
            <li>
              <h2>Windows</h2>
              <p>Install from the Microsoft Store. There is no public sideload package.</p>
              <p className="download-actions">
                <a className="text-link" href={windowsStoreUrl}>
                  Microsoft Store
                </a>
              </p>
            </li>
            <li>
              <h2>iOS</h2>
              <p>Native app, not in a public store yet. Build from source if you need it today.</p>
              <p className="download-actions">
                <a className="text-link" href={githubRepoUrl} target="_blank" rel="noreferrer">
                  Source on GitHub
                </a>
              </p>
            </li>
          </ul>

          <p className="download-checksums">
            <a className="text-link" href={release.checksumsUrl}>
              SHA256 checksums
            </a>
            {" · "}
            <a className="text-link" href={release.tagUrl}>
              GitHub {release.tag}
            </a>
            {" · "}
            <a className="text-link" href={githubLatestUrl}>
              Latest release
            </a>
          </p>
        </div>
      </section>
    </main>
  );
}

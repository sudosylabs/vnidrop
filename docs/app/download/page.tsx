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
import "./download.css";

export const metadata: Metadata = {
  title: "Download",
  description:
    "Beta builds of VniDrop for macOS, Linux, Android, and Windows. Early development; iOS is not in a public store yet.",
};

function FileLink({ asset }: { asset: ReleaseAsset }) {
  return (
    <a className="file-link" href={asset.url} rel="noreferrer">
      {asset.name}
      <span className="download-meta"> {formatBytes(asset.bytes)}</span>
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
          <p className="download-kicker">
            {channelLabel} · {release.tag}
          </p>
          <h1>Download</h1>
          <p>
            Public builds from GitHub Releases. Windows is on the Microsoft Store. iOS is not in a
            store yet.
          </p>
        </div>
      </section>

      <section className="download-list-section">
        <div className="page-shell">
          <ul className="download-list">
            <li id="macos">
              <h2>macOS</h2>
              <p>Notarized disk image. The app can update itself after install.</p>
              <p className="download-actions">
                {release.dmg ? <FileLink asset={release.dmg} /> : <span>No disk image in this release.</span>}
              </p>
              <CopyCommand command={homebrewInstall} />
            </li>
            <li id="linux">
              <h2>Linux</h2>
              <p>64-bit packages for Debian/Ubuntu and Fedora/RHEL.</p>
              <p className="download-actions">
                {release.deb ? <FileLink asset={release.deb} /> : null}
                {release.rpm ? <FileLink asset={release.rpm} /> : null}
                {!release.deb && !release.rpm ? <span>No Linux packages in this release.</span> : null}
              </p>
            </li>
            <li id="android">
              <h2>Android</h2>
              <p>Play-signed APK for sideload. The Play listing is still in closed testing.</p>
              <p className="download-actions">
                {release.apk ? <FileLink asset={release.apk} /> : <span>No Android APK in this release.</span>}
              </p>
            </li>
            <li id="windows">
              <h2>Windows</h2>
              <p>Install from the Microsoft Store. There is no public sideload package.</p>
              <p className="download-actions">
                <a className="text-link" href={windowsStoreUrl} rel="noreferrer">
                  Microsoft Store
                </a>
              </p>
            </li>
            <li id="ios">
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

import type { Metadata } from "next";
import Link from "next/link";
import { githubRepoUrl } from "@/lib/release";
import { DocumentToc } from "@/components/document-toc";
import styles from "@/components/document-page.module.css";
import guideStyles from "./page.module.css";

export const metadata: Metadata = {
  title: "Guide",
  description: "Install VniDrop, try previews, transfer files, connect saved devices, configure relays, and troubleshoot problems.",
};

const sections = [
  ["install", "Choose a build"], ["previews", "Try a preview"],
  ["transfer", "Send and receive"], ["saved-devices", "Saved devices"],
  ["network", "Network settings"], ["help", "Troubleshooting"], ["source", "Build from source"],
] as const;

export default function GuidePage() {
  return (
    <main id="main-content" className={styles.page}>
      <section className={styles.hero}>
        <div className={`${styles.heroInner} page-shell`}>
          <h1>VniDrop guide</h1>
          <p>Installation, everyday use, and help when two devices need a little more introduction.</p>
          <p className={styles.meta}>Installation · Transfers · Previews</p>
        </div>
      </section>

      <section className={styles.body}>
        <div className={`${styles.layout} page-shell`}>
          <DocumentToc sections={sections} label="Guide sections" />

          <article className={styles.article}>
            <div className={styles.callout}>
              <strong>Your first transfer. Your next build.</strong>
              <p>Start on the <Link href="/download/">download page</Link>. The current public release has versioned GitHub downloads, with a Microsoft Store option for Windows. Previews offer earlier changes for testing.</p>
            </div>
            <section id="install" className={styles.section}>
              <h2>Choose a build</h2>
              <ul>
                <li><strong>macOS:</strong> open the signed, notarized DMG and drag VniDrop into Applications. Direct release builds can check for updates; Homebrew is also available.</li>
                <li><strong>Windows:</strong> install through the Microsoft Store, or run the direct EXE. Direct installers are unsigned, so Windows may show a SmartScreen warning. Check the release and its checksum before running it.</li>
                <li><strong>Linux:</strong> use the DEB or RPM for your distribution. Install the downloaded file with <code>sudo apt install ./filename.deb</code> or <code>sudo dnf install ./filename.rpm</code> so dependencies are resolved.</li>
                <li><strong>Android:</strong> open the APK and allow installation from the browser or file manager when Android asks. You can turn that permission off afterward.</li>
                <li><strong>iOS / iPadOS:</strong> build with Xcode using the <a href={`${githubRepoUrl}/blob/master/apple/README.md`}>Apple setup guide</a>. The preview workflow does not distribute an iOS app.</li>
              </ul>
              <p>Release notes describe the files you are downloading. The source on master may be newer: older Windows and Linux releases used Compose, while current builds use native WinUI and GTK frontends.</p>
            </section>

            <section id="previews" className={styles.section}>
              <h2>Try a preview</h2>
              <p>Previews use Release builds and skip Microsoft Store, App Store, and Play Store submission. macOS builds are still signed and notarized. Each preview has its own release notes, five download files, a build manifest, and SHA256 checksums.</p>
              <p><Link className="text-link" href="/download/#preview">Find the latest preview →</Link></p>
              <h3>Installing and updating</h3>
              <p>Download subsequent previews manually. Preview releases do not update the normal macOS update feed or Homebrew cask, and they do not replace the normal release links.</p>
              <ul>
                <li><strong>Android previews install separately.</strong> They use a dedicated signing key and app identity, with their own history, settings, and saved devices. Install a newer preview over the previous one to retain that data.</li>
                <li><strong>Desktop previews use the existing app identity and data.</strong> Treat them as a replacement for your direct installation. They keep the base product version, so installing a different preview with the same version can require reinstalling the package. Close the app before switching builds.</li>
              </ul>
              <p>The preview number in the filename and release notes identifies the build. Keep it when reporting a problem. During early development, a newer build may change stored data; returning to an older release is not guaranteed to work with that data.</p>
            </section>

            <section id="transfer" className={styles.section}>
              <h2>Send and receive</h2>
              <ol>
                <li>Open VniDrop on both devices. On the sender, choose files, a batch, or a folder.</li>
                <li>Create an invitation and share the <code>.vnd</code> file with the receiver. QR codes are also available where the platform and hardware support them.</li>
                <li>On the receiver, open the invitation in VniDrop and request the transfer.</li>
                <li>Approve the request on the sender. The receiver saves to its selected destination; Android defaults to Downloads.</li>
                <li>Keep both devices reachable until the transfer finishes. Stop sharing when you no longer want the invitation to work.</li>
              </ol>
              <p>Files stream over an authenticated, end-to-end encrypted connection. Existing files are not silently overwritten. An invitation is a private access link: keep it between the people you intend to share with.</p>
              <p>Approval is required by default. Choosing “Anyone with this transfer” lets anyone holding the invitation receive while the share remains active.</p>
            </section>

            <section id="saved-devices" className={styles.section}>
              <h2>Saved devices</h2>
              <p>After a completed transfer, both devices can agree to remember each other. Next time, select that saved device to offer files without creating another invitation. The receiver still accepts each offer.</p>
              <p>A saved device represents an app installation. Clearing its data or creating a new identity means saving it again. An Android preview is a separate installation from the normal Android app.</p>
              <p>Both devices must be reachable to negotiate a transfer. There is no account, cloud inbox, or server queue that delivers files to an offline device. You can forget or block a saved device locally.</p>
            </section>

            <section id="network" className={styles.section}>
              <h2>Network settings</h2>
              <p>Automatic mode is the default. VniDrop tries a direct connection and can use a relay when networks prevent one. A relay forwards encrypted traffic; it does not keep a hosted copy of the files.</p>
              <dl className={styles.detailList}>
                <div><dt>Automatic</dt><dd>Public relay and discovery services, with direct connections whenever possible.</dd></div>
                <div><dt>Strict custom</dt><dd>Only your configured HTTPS relays and direct connections. Startup fails if no configured relay can be established.</dd></div>
                <div><dt>Custom with direct fallback</dt><dd>Prefer configured relays; continue with direct connections if they are unavailable.</dd></div>
                <div><dt>Local only</dt><dd>Disable relays and public discovery. This is direct-only networking, not a guarantee that connections stay inside a LAN.</dd></div>
              </dl>
              <p>The three restricted modes do not fall back to public relays or public discovery. Configure compatible settings on both devices.</p>
              <p>Stop active shares and transfers before applying a network change. The app tests the configuration and restores the previous one if it cannot connect. Create a fresh invitation after changing the relay configuration.</p>
              <p>For custom relay limits, certificates, and authentication, see the <a href={`${githubRepoUrl}/blob/master/README.md#custom-relay-servers`}>relay setup reference</a>.</p>
            </section>

            <section id="help" className={styles.section}>
              <h2>When something gets stuck</h2>
              <dl className={styles.detailList}>
                <div><dt>The other device cannot connect</dt><dd>Keep both apps open, confirm the share is still active, and check the network mode on both devices. With Local only, both devices need a reachable direct path. Try a new invitation after a network change.</dd></div>
                <div><dt>A transfer is waiting</dt><dd>Check the sender for an invitation approval request, or the receiver for a saved-device offer. Allow notifications if you want to see requests while another app is in front.</dd></div>
                <div><dt>Files cannot be saved</dt><dd>Check available space and destination permissions, then choose an accessible folder. On Android, reselect a custom destination if its folder permission was revoked.</dd></div>
                <div><dt>The native Linux app cannot open its credential store</dt><dd>Use a desktop session with a running, unlocked Secret Service provider such as GNOME Keyring. Saved-device credentials use the system keyring.</dd></div>
              </dl>
              <h3>Report a bug</h3>
              <p>Use “Report a bug” in Settings, About, or the app menu. Include both platform versions, the preview number if applicable, what you did, and what you expected. Contact details and recent logs are optional.</p>
              <p>Reports are sent only when you submit them. Transfer contents, invitations, and paths are excluded by design; avoid typing private invitations or file contents into the report yourself. See the <Link href="/privacy/#bug-reports">privacy policy</Link> for details.</p>
              <p>You can also <a href={`${githubRepoUrl}/issues`}>open a GitHub issue</a>. Report security problems through the <a href={`${githubRepoUrl}/blob/master/SECURITY.md`}>private security process</a>.</p>
            </section>

            <section id="source" className={styles.section}>
              <h2>Build from source</h2>
              <p>Use these entry points from the repository root after installing the prerequisites in each platform guide. They describe the current source, which may be ahead of published downloads.</p>
              <dl className={`${styles.detailList} ${guideStyles.builds}`}>
                <div><dt><a href={`${githubRepoUrl}/blob/master/windows/README.md`}>Windows · C# / WinUI 3</a></dt><dd><code>pwsh windows/scripts/build.ps1 -Test -Run</code><p>PowerShell, .NET, Windows SDK, Rust, and Bun. No Java runtime.</p></dd></div>
                <div><dt><a href={`${githubRepoUrl}/blob/master/linux/README.md`}>Linux · Rust / GTK 4 / libadwaita</a></dt><dd><code>make run-linux</code><p>GTK 4.10+, libadwaita 1.5+, Rust, and an unlocked Secret Service. No JVM.</p></dd></div>
                <div><dt><a href={`${githubRepoUrl}/blob/master/apple/README.md`}>Apple · SwiftUI</a></dt><dd><code>make open-apple APPLE_CODE_SIGNING=YES</code><p>Configure Local.xcconfig first. Use Xcode on macOS for Apple targets.</p></dd></div>
                <div><dt><a href={`${githubRepoUrl}/blob/master/CONTRIBUTING.md`}>Android · Kotlin / Compose</a></dt><dd><code>make build-android</code><p>A local debug build for development; requires JDK, Android SDK/NDK, Rust, and Bun.</p></dd></div>
              </dl>
              <p>For repository checks and contributions, see <a href={`${githubRepoUrl}/blob/master/CONTRIBUTING.md`}>CONTRIBUTING.md</a>.</p>
            </section>
          </article>
        </div>
      </section>
    </main>
  );
}

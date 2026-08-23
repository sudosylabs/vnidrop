import type { Metadata } from "next";
import styles from "./page.module.css";

export const metadata: Metadata = {
  title: "Privacy policy",
  description:
    "How VniDrop handles transfers, local app data, optional bug reports, and website visits.",
};

const sections = [
  ["scope", "Scope"],
  ["transfers", "Transfers"],
  ["local-data", "Local data"],
  ["bug-reports", "Bug reports"],
  ["website", "Website"],
  ["permissions", "Permissions"],
  ["providers", "Service providers"],
  ["retention", "Retention"],
  ["choices", "Your choices"],
  ["security", "Security"],
  ["changes", "Changes"],
  ["contact", "Contact"],
];

export default function PrivacyPage() {
  return (
    <main id="main-content" className={styles.privacyPage}>
      <section className={styles.privacyHero}>
        <div className={`${styles.privacyHeroInner} page-shell`}>
          <h1>Privacy Policy</h1>
          <p>
            This policy explains what moves between devices, what stays local, and what is sent
            only when you choose to submit a bug report.
          </p>
          <p className={styles.privacyMeta}>Effective August 23, 2026 · Version 1.3</p>
        </div>
      </section>

      <section className={styles.privacyDocumentSection}>
        <div className={`${styles.privacyDocumentLayout} page-shell`}>
          <aside className={styles.privacyToc}>
            <p>On this page</p>
            <nav aria-label="Privacy policy sections">
              <ol>
                {sections.map(([id, label]) => (
                  <li key={id}>
                    <a href={`#${id}`}>{label}</a>
                  </li>
                ))}
              </ol>
            </nav>
          </aside>

          <article className={styles.privacyDocument}>
            <div className={styles.privacyCallout}>
              <strong>The short version</strong>
              <p>
                VniDrop has no user accounts and does not upload your transfer to a VniDrop file
                store. Files travel over an authenticated, end-to-end encrypted connection.
                VniDrop has no telemetry or analytics; a bug report is sent only when you submit one.
              </p>
            </div>

            <section id="scope" className={styles.policySection}>
              <h2>Scope and who “VniDrop” means</h2>
              <p>
                This policy covers the official VniDrop website, the VniDrop applications for
                Android, iOS, macOS, Windows, and Linux, and the bug-report service configured by
                the official project. For an official release, VniDrop’s data controller is the
                individual publisher named in the applicable app-store listing. In this policy,
                “VniDrop,” “we,” and “us” also include the maintainers acting on that publisher’s
                behalf. The publisher can be reached at support@sudosy.fr.
              </p>
              <p>
                VniDrop is open-source software. A build distributed or operated by someone else
                may use different networking infrastructure, bug-report settings, or website
                hosting. That distributor is responsible for explaining its own practices.
              </p>
            </section>

            <section id="transfers" className={styles.policySection}>
              <h2>What happens during a transfer</h2>
              <h3>File contents</h3>
              <p>
                The sender chooses files or folders on their device. VniDrop streams those bytes to
                an approved receiver and does not first upload them to a VniDrop-hosted storage
                bucket. The receiver saves the files to a destination they choose. Relayed traffic
                remains end-to-end encrypted.
              </p>
              <h3>Invitations and transfer metadata</h3>
              <p>
                A QR code, NFC tag, or <code>.vnd</code> file contains a transfer invitation. The
                invitation includes connection and content identifiers plus transfer metadata such
                as the transfer name, optional sender name, creation time, file count, and total
                size. It is a capability: anyone who receives it may be able to request the transfer
                while the share is active. Treat it like a private access link.
              </p>
              <h3>What peers and relays can see</h3>
              <p>
                A receive request can disclose the receiver’s chosen display or device name,
                application version, and a technical endpoint identifier to the sender. A direct
                connection exposes the peers’ IP addresses to one another. When a public relay is
                used, its operator can observe connection metadata such as source and destination IP
                addresses, connection time, and the amount of relayed data, but cannot read the
                encrypted transfer contents.
              </p>
              <div className={styles.policyNote}>
                <p>
                  Approval is required by default. If the sender selects “Anyone with this transfer,”
                  anyone holding the invitation may receive the files until sharing stops.
                </p>
              </div>
            </section>

            <section id="local-data" className={styles.policySection}>
              <h2>Information kept on your device</h2>
              <p>VniDrop stores the information needed to operate the app locally, including:</p>
              <ul>
                <li>device identity and networking keys used to establish secure connections;</li>
                <li>active shares, transfer history, receiver requests, progress, and status;</li>
                <li>app preferences, including access choices;</li>
                <li>download destinations and locally managed transfer data; and</li>
                <li>an anonymous installation identifier used only for bug-report correlation.</li>
              </ul>
              <p>
                This information remains until you remove the relevant history, stop or delete a
                share, clear the app’s data, or uninstall the app, subject to operating-system file
                behavior. Removing VniDrop history does not delete a file you already downloaded;
                delete that file through your operating system if you no longer want it.
              </p>
            </section>

            <section id="bug-reports" className={styles.policySection}>
              <h2>Optional bug reports</h2>
              <p>
                VniDrop has no automatic telemetry, usage analytics, or crash auto-reporting.
                Nothing is sent to a bug-report service unless you explicitly submit a report.
              </p>
              <h3>User-submitted bug reports</h3>
              <p>
                A bug report is sent only when you press submit. It can contain what you say
                happened, what you expected, reproduction steps, an optional contact email, app and
                platform versions, an anonymous installation ID, device name and model, operating
                system, network and battery information, and optional recent logs. You can exclude
                logs before submitting.
              </p>
              <h3>Data deliberately excluded</h3>
              <p>
                Bug reports are designed to exclude transfer contents, invitations, and file paths.
                Before optional logs are sent, VniDrop applies rules intended to redact invitation
                tokens, endpoint identifiers, absolute paths, file and content URIs, and platform
                document identifiers. No redaction system is perfect, so review anything you type
                into a bug report and avoid including secrets.
              </p>
            </section>

            <section id="website" className={styles.policySection}>
              <h2>The VniDrop website</h2>
              <p>
                This website is a static product site. It does not provide an account, contact form,
                advertising, behavioral analytics, marketing pixels, or non-essential cookies. It
                does not ask the browser for access to your files, camera, contacts, location, or
                nearby devices.
              </p>
              <p>
                GitHub Pages hosts the static site, while Cloudflare proxies requests and provides
                DNS and security services for the domain. They may process routine request
                information—such as IP address, time, requested page, referrer, and browser user
                agent—to deliver the site, maintain reliability, and prevent abuse.
              </p>
            </section>

            <section id="permissions" className={styles.policySection}>
              <h2>Device permissions</h2>
              <dl className={styles.permissionList}>
                <div>
                  <dt>Files &amp; folders</dt>
                  <dd>Choose what to send and where received files are saved.</dd>
                </div>
                <div>
                  <dt>Camera / scanner</dt>
                  <dd>Scan a QR invitation when you choose that receive method.</dd>
                </div>
                <div>
                  <dt>NFC</dt>
                  <dd>Read or write an invitation through a compatible NFC tag.</dd>
                </div>
                <div>
                  <dt>Network &amp; notifications</dt>
                  <dd>Connect peers and alert you to background receiver requests.</dd>
                </div>
              </dl>
              <p>
                VniDrop requests a platform permission only for the related feature. On Android, QR
                scanning may be provided through Google Play services Code Scanner. Platform-level
                permission prompts and service-provider terms also apply.
              </p>
            </section>

            <section id="providers" className={styles.policySection}>
              <h2>Infrastructure and external services</h2>
              <dl className={styles.providerList}>
                <div>
                  <dt>Iroh / public relay operators</dt>
                  <dd>
                    Device discovery, connection establishment, and encrypted relay fallback.
                    Relays process connection metadata but cannot decrypt transfer contents.
                  </dd>
                </div>
                <div>
                  <dt>Cloudflare</dt>
                  <dd>
                    Proxies website requests and provides DNS, security, and abuse controls. When
                    the optional bug-report service is configured, it uses Cloudflare Workers, D1,
                    and R2.
                  </dd>
                </div>
                <div>
                  <dt>Google Play services</dt>
                  <dd>
                    May provide the QR code scanner on supported Android devices when you choose to
                    scan an invitation.
                  </dd>
                </div>
                <div>
                  <dt>GitHub / GitHub Pages</dt>
                  <dd>
                    Hosts the source repository, issue tracker, static VniDrop website, and external
                    pages linked from this site.
                  </dd>
                </div>
              </dl>
              <p className={styles.providerLinks}>
                Provider policies:{" "}
                <a
                  href="https://services.iroh.computer/legal/privacy"
                  target="_blank"
                  rel="noreferrer"
                >
                  Iroh
                </a>
                ,{" "}
                <a
                  href="https://www.cloudflare.com/policies/privacy/"
                  target="_blank"
                  rel="noreferrer"
                >
                  Cloudflare
                </a>
                ,{" "}
                <a href="https://policies.google.com/privacy" target="_blank" rel="noreferrer">
                  Google
                </a>
                , and{" "}
                <a
                  href="https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement"
                  target="_blank"
                  rel="noreferrer"
                >
                  GitHub
                </a>
                .
              </p>
            </section>

            <section id="retention" className={styles.policySection}>
              <h2>Retention and deletion</h2>
              <div className={styles.retentionTableWrap}>
                <table className={styles.retentionTable}>
                  <caption className="sr-only">Data retention periods</caption>
                  <thead>
                    <tr>
                      <th scope="col">Data</th>
                      <th scope="col">Typical retention</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr>
                      <th scope="row">Transfer history and settings</th>
                      <td>Until you delete them, clear app data, or uninstall</td>
                    </tr>
                    <tr>
                      <th scope="row">Server bug reports</th>
                      <td>The current project configuration is 90 days, with scheduled deletion</td>
                    </tr>
                    <tr>
                      <th scope="row">Downloaded files</th>
                      <td>Until you delete them through your operating system</td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <p>
                Operational backups, provider logs, and deletion backlogs may persist briefly beyond
                the stated period where necessary for security, integrity, or legal obligations. If
                the production bug-report retention configuration changes, this policy should be
                updated to match it.
              </p>
            </section>

            <section id="choices" className={styles.policySection}>
              <h2>Your choices and rights</h2>
              <ul>
                <li>
                  Submit a bug report only when you choose, omit contact information, and exclude
                  logs.
                </li>
                <li>Approve or refuse each receiver, cancel a transfer, or stop sharing.</li>
                <li>
                  Delete individual transfer history or clear completed, failed, and cancelled
                  receive history.
                </li>
                <li>
                  Delete downloaded files using your operating system, or clear all app data by
                  uninstalling or resetting the app.
                </li>
              </ul>
              <p>
                Depending on where you live, privacy law may provide rights to access, correct,
                delete, restrict, or object to processing of personal information. Because VniDrop
                has no account and bug reports use an anonymous installation ID, we may
                not be able to connect a server record to you without additional information. Use
                the contact method below and provide only what is needed to locate your submission.
              </p>
            </section>

            <section id="security" className={styles.policySection}>
              <h2>Security</h2>
              <p>
                VniDrop uses authenticated end-to-end encrypted connections, content verification,
                deny-by-default share access, bounded bug-report payloads, redaction, and safe file
                publishing that avoids silently replacing an existing file. No system can guarantee
                absolute security. Keep invitations private, verify receiver names, keep your device
                updated, and stop sharing when a transfer is finished.
              </p>
              <p>
                Please report a suspected vulnerability through the private process in the{" "}
                <a
                  href="https://github.com/vnidrop/vnidrop/blob/master/SECURITY.md"
                  target="_blank"
                  rel="noreferrer"
                >
                  VniDrop security policy
                </a>
                , not in a public issue.
              </p>
            </section>

            <section id="changes" className={styles.policySection}>
              <h2>Changes to this policy</h2>
              <p>
                VniDrop is in early development. Features and data practices may change. When this
                policy changes, we will update the effective date and version at the top of the page
                and publish the revised text with the project. Material changes should be called out
                in release notes or the application where practical.
              </p>
            </section>

            <section id="contact" className={`${styles.policySection} ${styles.policyContact}`}>
              <h2>Contact</h2>
              <p>
                For a privacy question, rights request, or support request, email
                support@sudosy.fr. Do not put an invitation, file content, credentials, or other
                sensitive information in a public issue.
              </p>
              <a className={styles.privacyContactLink} href="mailto:support@sudosy.fr">
                Email support@sudosy.fr
              </a>
            </section>
          </article>
        </div>
      </section>
    </main>
  );
}

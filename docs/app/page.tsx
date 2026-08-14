import Image from "next/image";
import Link from "next/link";
import type { ReactNode } from "react";

const steps: Array<{ number: string; title: string; text: ReactNode }> = [
  {
    number: "1",
    title: "Choose what to send",
    text: "Pick files, a batch, or a folder. VniDrop keeps the original folder structure.",
  },
  {
    number: "2",
    title: "Introduce the devices",
    text: (
      <>
        The first time, share a small invitation—a QR code, an NFC tag, or a <code>.vnd</code> file.
        After you both remember the other device, you can send to it directly from Saved devices.
      </>
    ),
  },
  {
    number: "3",
    title: "Confirm the handoff",
    text: "An invitation still asks you to approve each receiver. A Saved device still has to accept the incoming offer. Remembering a device never auto-receives.",
  },
  {
    number: "4",
    title: "Stream and verify",
    text: "Files move over an authenticated, encrypted connection. Content addressing checks that the received bytes match what you sent. You can cancel or stop sharing at any time.",
  },
];

const does = [
  "Connect devices directly when a path exists.",
  "Forward the same encrypted connection through a relay when it does not. The relay is a route, not storage.",
  "Verify content on arrival so the received bytes match exactly what you sent.",
  "Ask before each download, unless you choose otherwise.",
  "Let you remember a device after a transfer so the next send does not need a new invitation. They still confirm the offer.",
];

const doesNot = [
  "Create accounts.",
  "Keep a hosted copy of the transfer.",
  "Collect telemetry or analytics. A bug report is sent only when you submit one.",
  "Replace a file that already exists at the destination.",
  "Receive files automatically because a device is saved.",
];

export default function HomePage() {
  return (
    <main id="main-content">
      <section className="hero">
        <div className="page-shell hero-inner">
          <p className="hero-status">Open source · Early development</p>
          <h1>
            Send files from this device to that one.
            <em>They ask. You decide.</em>
          </h1>
          <p className="hero-lead">
            VniDrop moves files and folders between devices without an account and without a cloud
            copy. Meet once with an invitation. After that, send to a Saved device. The receiver
            still confirms every transfer.
          </p>
          <p className="hero-actions">
            <Link className="text-link" href="/download/">
              Download
            </Link>
            <a className="text-link" href="#how-it-works">
              How a transfer works
            </a>
          </p>
          <p className="hero-platforms">
            Available on Android, iOS, macOS, Windows, and Linux.
          </p>
          <div className="hero-photos">
            <figure className="hero-photo">
              <Image
                src="/shots/desktop-review.png"
                width={1920}
                height={1080}
                sizes="(max-width: 720px) calc(100vw - 32px), 720px"
                alt="VniDrop on Windows, reviewing a transfer with ask-before-each-download selected."
                priority
                unoptimized
              />
              <figcaption>Desktop on Windows. Android and Linux share this interface.</figcaption>
            </figure>
            <figure className="shot shot-phone">
              <Image
                src="/shots/share-securely.png"
                width={1320}
                height={2868}
                sizes="(max-width: 720px) 260px, 240px"
                alt="Native iPhone share sheet with a QR code and an option to write the invitation to an NFC tag."
                priority
                unoptimized
              />
              <figcaption>Native iPhone app.</figcaption>
            </figure>
          </div>
        </div>
      </section>

      <section id="how-it-works" className="manual">
        <div className="page-shell manual-layout">
          <div className="manual-copy">
            <h2>How a transfer works</h2>
            <p>
              An invitation introduces devices that have never met. A Saved device is one you both
              chose to remember after a transfer.
            </p>
            <ol className="manual-steps">
              {steps.map((step) => (
                <li key={step.number}>
                  <span className="manual-num" aria-hidden="true">
                    {step.number}
                  </span>
                  <div>
                    <h3>{step.title}</h3>
                    <p>{step.text}</p>
                  </div>
                </li>
              ))}
            </ol>
          </div>
          <div className="manual-photos">
            <figure className="shot">
              <Image
                src="/shots/send-anywhere.png"
                width={1320}
                height={2868}
                sizes="(max-width: 800px) calc(100vw - 64px), 280px"
                alt="Native iPhone transfer details, with ask-before-each-download enabled."
                unoptimized
              />
              <figcaption>Transfer details on iPhone.</figcaption>
            </figure>
            <figure className="shot">
              <Image
                src="/shots/choose-receivers.png"
                width={1320}
                height={2868}
                sizes="(max-width: 800px) calc(100vw - 64px), 280px"
                alt="Receive request from a Mac mini, with Approve and Refuse actions."
                unoptimized
              />
              <figcaption>A receiver asks first. You approve or refuse.</figcaption>
            </figure>
          </div>
        </div>
      </section>

      <section className="facts" aria-labelledby="facts-heading">
        <div className="page-shell">
          <h2 id="facts-heading">What VniDrop does—and doesn’t.</h2>
          <div className="facts-grid">
            <div>
              <h3>Does</h3>
              <ul>
                {does.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </div>
            <div>
              <h3>Does not</h3>
              <ul>
                {doesNot.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </div>
          </div>
          <p className="facts-more">
            <Link className="text-link" href="/privacy/">
              Read the privacy policy
            </Link>
          </p>
        </div>
      </section>
    </main>
  );
}

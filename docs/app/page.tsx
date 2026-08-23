import Image from "next/image";
import Link from "next/link";
import { githubRepoUrl } from "@/lib/release";

const traits = [
  {
    title: "Every platform in the room",
    text: "Android, iOS, macOS, Windows, and Linux. Native SwiftUI on Apple. Compose on the rest.",
    icon: (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="2.5" y="4.5" width="12" height="9" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <rect x="10.5" y="10.5" width="11" height="9" rx="2" fill="none" stroke="currentColor" strokeWidth="1.5" />
      </svg>
    ),
  },
  {
    title: "No account. No hosted copy.",
    text: "No signup. Files move on an authenticated, encrypted connection. A relay is only a route when there is no direct path — it never stores the transfer.",
    icon: (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M7.5 16.5H7a4 4 0 1 1 .6-7.95A5 5 0 0 1 17.5 10H18a3.5 3.5 0 0 1 0 7h-.5"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
        <path d="M9 20.5 16.5 9.5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      </svg>
    ),
  },
  {
    title: "You stay in control",
    text: "Ask before each download, or open a transfer to anyone with the invitation. Cancel or stop sharing at any time. Existing files at the destination are not overwritten.",
    icon: (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M12 3.5 19.5 7v5.2c0 4.3-3.1 7.4-7.5 8.8C7.6 19.6 4.5 16.5 4.5 12.2V7L12 3.5Z"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinejoin="round"
        />
        <path d="M8.5 12.2 11 14.7l4.5-5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    ),
  },
  {
    title: "Meet with an invitation",
    text: "The first meeting is a QR code, an NFC tag, or a .vnd file. After that you can send to a Saved device.",
    icon: (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M5 5h5v5H5zM14 5h5v5h-5zM5 14h5v5H5z" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <path d="M14 14h2.5M18.5 14H21M14 17.5h7M14 21h3.5M19.5 21H21" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      </svg>
    ),
  },
  {
    title: "Then Saved devices",
    text: "Both of you choose to remember the other after a transfer. A Saved device still has to accept the offer. Remembering a device never auto-receives.",
    icon: (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="6.5" y="3.5" width="11" height="17" rx="2" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <path d="M10 6.5h4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        <circle cx="12" cy="16.5" r="1" fill="currentColor" />
      </svg>
    ),
  },
  {
    title: "Open source",
    text: "Apache 2.0. The runtime, the apps, and this site are on GitHub. Early development — still a beta.",
    icon: (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="12" r="8.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <path d="M12 3.5v17M3.5 12h17" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <path d="M12 3.5c2.8 3.2 4.2 6.2 4.2 8.5S14.8 17.3 12 20.5C9.2 17.3 7.8 14.3 7.8 12S9.2 6.7 12 3.5Z" fill="none" stroke="currentColor" strokeWidth="1.5" />
      </svg>
    ),
  },
];

const steps = [
  {
    src: "/shots/choose-what-to-share.png",
    width: 1320,
    height: 2868,
    alt: "Native iPhone new-transfer sheet with options to choose files or a folder.",
    title: "Choose what to send",
    text: "Files, a batch, or a folder. The original folder structure stays intact.",
  },
  {
    src: "/shots/share-securely.png",
    width: 1320,
    height: 2868,
    alt: "Native iPhone share sheet with a QR code for the transfer invitation.",
    title: "Introduce the devices",
    text: "Show a QR code, write an NFC tag, or save a .vnd invitation. The other device opens it in VniDrop.",
  },
  {
    src: "/shots/choose-receivers.png",
    width: 1320,
    height: 2868,
    alt: "Receive request on iPhone, with Approve and Refuse actions.",
    title: "They still decide",
    text: "An invitation asks you to approve each receiver. A Saved device still has to accept. Nothing auto-receives.",
  },
];

const questions = [
  {
    q: "Do I need an account?",
    a: "No. There is no signup and no login. Devices meet with an invitation, or you send to a Saved device.",
  },
  {
    q: "Does a copy sit on a server?",
    a: "No. Bytes stream between the two devices. If they cannot reach each other directly, a relay forwards the connection. It does not keep the files.",
  },
  {
    q: "What is a Saved device?",
    a: "After a transfer, both sides can choose to remember the other. Later you can send to that device without a new invitation. They still confirm every time.",
  },
  {
    q: "Which platforms?",
    a: "Android, iOS, macOS, Windows, and Linux. Windows is on the Microsoft Store. macOS, Linux, and Android builds are on GitHub. iOS is not in a public store yet.",
  },
];

const platforms = [
  { href: "/download/#macos", label: "macOS" },
  { href: "/download/#linux", label: "Linux" },
  { href: "/download/#android", label: "Android" },
  { href: "/download/#windows", label: "Windows" },
  { href: "/download/#ios", label: "iOS" },
];

export default function HomePage() {
  return (
    <main id="main-content">
      <section className="hero">
        <div className="page-shell hero-copy">
          <p className="hero-kicker">Open source file transfer</p>
          <h1>Send files from this device to that one.</h1>
          <p className="hero-lead">
            No cloud folder. No signup. Meet with an invitation, then send to a Saved device. The
            receiver still confirms every time.
          </p>
          <p className="hero-actions">
            <Link className="btn btn-primary" href="/download/">
              Download
            </Link>
            <a className="text-link" href="#how-it-works">
              How a transfer works
            </a>
          </p>
          <p className="hero-platforms">Android · iOS · macOS · Windows · Linux</p>
        </div>
        <figure className="hero-figure">
          <Image
            src="/shots/hero.jpg"
            width={1024}
            height={819}
            sizes="(max-width: 1100px) calc(100vw - 32px), 1024px"
            alt="VniDrop on macOS reviewing a transfer, and on iPhone choosing how to connect: a .vnd file, a QR code, or NFC."
            priority
            unoptimized
          />
        </figure>
      </section>

      <section className="traits" aria-labelledby="traits-heading">
        <div className="page-shell">
          <h2 id="traits-heading">Built for a handoff, not a cloud.</h2>
          <ul className="traits-grid">
            {traits.map((trait) => (
              <li key={trait.title}>
                <span className="trait-icon">{trait.icon}</span>
                <h3>{trait.title}</h3>
                <p>{trait.text}</p>
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section id="how-it-works" className="how">
        <div className="page-shell">
          <h2>How a transfer works</h2>
          <p className="section-lead">
            An invitation introduces devices that have never met. A Saved device is one you both
            chose to remember after a transfer.
          </p>
          <ol className="how-list">
            {steps.map((step) => (
              <li key={step.title} className="how-row">
                <div className="how-copy">
                  <h3>{step.title}</h3>
                  <p>{step.text}</p>
                </div>
                <figure className="shot-phone">
                  <Image
                    src={step.src}
                    width={step.width}
                    height={step.height}
                    sizes="(max-width: 800px) 240px, 280px"
                    alt={step.alt}
                    unoptimized
                  />
                </figure>
              </li>
            ))}
          </ol>
        </div>
      </section>

      <section className="faq" aria-labelledby="faq-heading">
        <div className="page-shell faq-inner">
          <h2 id="faq-heading">Questions</h2>
          <dl className="faq-list">
            {questions.map((item) => (
              <div key={item.q}>
                <dt>{item.q}</dt>
                <dd>{item.a}</dd>
              </div>
            ))}
          </dl>
        </div>
      </section>

      <section className="get">
        <div className="page-shell get-inner">
          <div>
            <h2>Get VniDrop</h2>
            <p>
              Beta builds for macOS, Linux, and Android. Windows is on the Microsoft Store. iOS is
              not in a public store yet.
            </p>
            <ul className="os-list">
              {platforms.map((platform) => (
                <li key={platform.label}>
                  <Link href={platform.href}>{platform.label}</Link>
                </li>
              ))}
            </ul>
          </div>
          <p className="get-actions">
            <Link className="btn btn-primary" href="/download/">
              Download
            </Link>
            <a className="text-link" href={githubRepoUrl} target="_blank" rel="noreferrer">
              Source on GitHub
            </a>
          </p>
        </div>
      </section>
    </main>
  );
}

import Image from "next/image";
import Link from "next/link";
import { githubRepoUrl } from "@/lib/release";
import styles from "./page.module.css";

const traits = [
  {
    title: "Every platform in the room",
    text: "Android, iOS, macOS, Windows, and Linux can meet without joining the same ecosystem.",
  },
  {
    title: "No hosted copy",
    text: "Files move over an authenticated, encrypted connection. A relay can route bytes but never stores the transfer.",
  },
  {
    title: "Approval by default",
    text: "The receiver confirms every request. You can cancel a transfer or stop sharing whenever you need to.",
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
      <section className={styles.hero}>
        <div className={`${styles.heroInner} page-shell`}>
          <div className={styles.heroCopy}>
            <p className={styles.heroRoute} aria-label="From this device to that one">
              <span>This device</span>
              <span className={styles.routeLine} aria-hidden="true" />
              <span>That one</span>
            </p>
            <h1>Send files from this device to that one.</h1>
            <p className={styles.heroLead}>
              Move a file, a folder, or a whole batch across Android, iOS, macOS, Windows, and Linux.
              No account. No hosted copy. The receiver approves it.
            </p>
            <p className={styles.heroActions}>
              <Link className="btn btn-primary" href="/download/">
                Choose your download
              </Link>
              <a className="text-link" href="#how-it-works">
                See the handoff
              </a>
            </p>
            <p className={styles.heroPlatforms}>Open source · Apache 2.0 · Early development</p>
          </div>
          <figure className={styles.heroFigure}>
            <Image
              src="/shots/hero.png"
              width={2400}
              height={1920}
              sizes="(max-width: 800px) calc(100vw - 32px), (max-width: 1200px) 58vw, 650px"
              alt="VniDrop on macOS reviewing a transfer, and on iPhone choosing how to connect: a .vnd file, a QR code, or NFC."
              priority
              unoptimized
            />
          </figure>
        </div>
      </section>

      <section className={styles.traits} aria-labelledby="traits-heading">
        <div className="page-shell">
          <h2 id="traits-heading">Built for a handoff, not a cloud.</h2>
          <ul className={styles.traitsGrid}>
            {traits.map((trait) => (
              <li key={trait.title}>
                <h3>{trait.title}</h3>
                <p>{trait.text}</p>
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section id="how-it-works" className={styles.how}>
        <div className="page-shell">
          <div className={styles.howIntro}>
            <h2>How a transfer works</h2>
            <p className={styles.sectionLead}>
              An invitation introduces devices that have never met. A Saved device is one you both
              chose to remember after a transfer.
            </p>
          </div>
          <ol className={styles.howList}>
            {steps.map((step) => (
              <li key={step.title} className={styles.howRow}>
                <div className={styles.howCopy}>
                  <h3>{step.title}</h3>
                  <p>{step.text}</p>
                </div>
                <figure className={styles.shotPhone}>
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

      <section className={styles.faq} aria-labelledby="faq-heading">
        <div className={`${styles.faqInner} page-shell`}>
          <h2 id="faq-heading">Questions</h2>
          <dl className={styles.faqList}>
            {questions.map((item) => (
              <div key={item.q}>
                <dt>{item.q}</dt>
                <dd>{item.a}</dd>
              </div>
            ))}
          </dl>
        </div>
      </section>

      <section className={styles.get}>
        <div className={`${styles.getInner} page-shell`}>
          <div>
            <h2>Make the next handoff.</h2>
            <p>
              Beta builds for macOS, Linux, and Android. Windows is on the Microsoft Store. iOS is
              not in a public store yet.
            </p>
            <ul className={styles.osList}>
              {platforms.map((platform) => (
                <li key={platform.label}>
                  <Link href={platform.href}>{platform.label}</Link>
                </li>
              ))}
            </ul>
          </div>
          <p className={styles.getActions}>
            <Link className="btn btn-primary" href="/download/">
              Choose a build
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

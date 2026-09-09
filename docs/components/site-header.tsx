import Link from "next/link";
import { Brand } from "@/components/brand";
import { githubRepoUrl } from "@/lib/release";
import styles from "./site-header.module.css";

export function SiteHeader() {
  return (
    <header className={styles.siteHeader}>
      <div className={`${styles.inner} page-shell`}>
        <Brand />
        <nav className={styles.nav} aria-label="Site">
          <Link className={styles.link} href="/guide/">
            Guide
          </Link>
          <Link className={`${styles.link} ${styles.download}`} href="/download/">
            Download
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden="true">
              <path
                d="M12 3v12m-5-5 5 5 5-5M5 17v4h14v-4"
                stroke="currentColor"
                strokeWidth="1.75"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          </Link>
          <a className={`${styles.link} ${styles.github}`} href={githubRepoUrl} target="_blank" rel="noreferrer">
            GitHub
          </a>
        </nav>
      </div>
    </header>
  );
}

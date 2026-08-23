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
          <Link className={`${styles.link} ${styles.how}`} href="/#how-it-works">
            How it works
          </Link>
          <Link className={`${styles.download} btn btn-primary`} href="/download/">
            Download
          </Link>
          <a className={styles.link} href={githubRepoUrl} target="_blank" rel="noreferrer">
            GitHub
          </a>
        </nav>
      </div>
    </header>
  );
}

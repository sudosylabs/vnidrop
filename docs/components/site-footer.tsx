import Link from "next/link";
import { githubRepoUrl } from "@/lib/release";
import styles from "./site-footer.module.css";

export function SiteFooter() {
  const year = new Date().getUTCFullYear();

  return (
    <footer className={styles.siteFooter}>
      <div className={`${styles.inner} page-shell`}>
        <p className={styles.identity}>
          <strong translate="no">VniDrop</strong>
          <span>© {year} contributors · Open source · Early development</span>
        </p>
        <nav className={styles.links} aria-label="Footer">
          <Link href="/download/">Download</Link>
          <Link href="/privacy/">Privacy</Link>
          <a href={githubRepoUrl} target="_blank" rel="noreferrer">
            GitHub
          </a>
          <a href={`${githubRepoUrl}/blob/master/LICENSE`} target="_blank" rel="noreferrer">
            Apache 2.0
          </a>
        </nav>
      </div>
    </footer>
  );
}

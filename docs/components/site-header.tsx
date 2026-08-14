import Link from "next/link";
import { Brand } from "@/components/brand";
import { githubRepoUrl } from "@/lib/release";

export function SiteHeader() {
  return (
    <header className="site-header">
      <div className="header-inner page-shell">
        <Brand />
        <nav className="header-nav" aria-label="Site">
          <Link href="/download/">Download</Link>
          <Link href="/privacy/">Privacy</Link>
          <a href={githubRepoUrl} target="_blank" rel="noreferrer">
            GitHub
          </a>
        </nav>
      </div>
    </header>
  );
}

import Link from "next/link";
import { Brand } from "@/components/brand";
import { githubRepoUrl } from "@/lib/release";

export function SiteHeader() {
  return (
    <header className="site-header">
      <div className="header-inner page-shell">
        <Brand />
        <nav className="header-nav" aria-label="Site">
          <Link className="header-how" href="/#how-it-works">
            How it works
          </Link>
          <Link className="btn btn-primary" href="/download/">
            Download
          </Link>
          <a href={githubRepoUrl} target="_blank" rel="noreferrer">
            GitHub
          </a>
        </nav>
      </div>
    </header>
  );
}
